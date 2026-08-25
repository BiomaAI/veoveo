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
  access: { administration: false, feature_read: false, feature_write: false, feature_publish: false },
  layers: [], publications: [], compositions: [], styles: new Map(),
  sources: [], datasets: {}, activeReleases: [], acquisitions: [], profiles: [],
  map: null, mapReady: false, mapLayers: [], renderedLayerIds: [], sourceIds: [],
  queryGeneration: 0, refreshTimer: null, pollTimer: null, closing: false,
};
const ACTIVE_ACQUISITION = new Set(["queued", "running", "cancel_requested"]);
const MAX_VIEW_FEATURES_PER_LAYER = 5000;
const PALETTE = ["#287e8e", "#b8683b", "#5a7d3c", "#725e9c", "#b34f68", "#3f70a5", "#99712d"];
const el = (id) => document.getElementById(id);

function setStatus(message, kind = "") {
  el("status").textContent = message;
  el("status").className = `status ${kind}`;
}

async function read(uri) {
  const result = await bridge.request("resources/read", { uri });
  const content = (result && result.contents || [])[0];
  if (!content || typeof content.text !== "string") throw new Error(`No JSON returned for ${uri}`);
  return JSON.parse(content.text);
}

async function tool(name, args) {
  const result = await bridge.request("tools/call", { name, arguments: args });
  if (result && (result.isError || result.is_error)) {
    const text = (result.content || []).find((value) => value.type === "text");
    throw new Error(text ? text.text : `${name} failed`);
  }
  return result && (result.structuredContent || result.structured_content) || result;
}

function uuid() {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  return Array.from(crypto.getRandomValues(new Uint8Array(16)),
    (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function node(tag, text, className) {
  const value = document.createElement(tag);
  if (text !== undefined) value.textContent = String(text);
  if (className) value.className = className;
  return value;
}

function replaceList(hostId, values, renderer, emptyText) {
  const host = el(hostId);
  host.replaceChildren();
  if (!values.length) {
    host.append(node("div", emptyText, "muted item"));
    return;
  }
  host.append(...values.map(renderer));
}

function renderAccess() {
  const permissions = [
    ["feature read", state.access.feature_read],
    ["feature write", state.access.feature_write],
    ["publish", state.access.feature_publish],
    ["admin", state.access.administration],
  ];
  el("access").replaceChildren(...permissions.map(([label, enabled]) =>
    node("span", label, `chip ${enabled ? "on" : ""}`)));
  el("map-permission").hidden = state.access.feature_read;
  el("map-workspace").hidden = !state.access.feature_read;
  el("layers-permission").hidden = state.access.feature_read;
  el("layers-workspace").hidden = !state.access.feature_read;
  el("data-permission").hidden = state.access.administration;
  el("data-workspace").hidden = !state.access.administration;
  document.querySelectorAll("[data-write-control]").forEach((control) => {
    control.hidden = !state.access.feature_write;
  });
  el("publish-layer").hidden = !state.access.feature_publish;
  document.querySelectorAll(".tab").forEach((tab) => {
    tab.disabled = tab.dataset.view === "data-view"
      ? !state.access.administration : !state.access.feature_read;
  });
  const selected = document.querySelector('.tab[aria-selected="true"]');
  if (selected.disabled) {
    const next = [...document.querySelectorAll(".tab")].find((tab) => !tab.disabled);
    if (next) selectView(next.dataset.view);
  }
}

function selectView(viewId) {
  document.querySelectorAll(".view").forEach((view) => { view.hidden = view.id !== viewId; });
  document.querySelectorAll(".tab").forEach((tab) => {
    tab.setAttribute("aria-selected", String(tab.dataset.view === viewId));
  });
  if (viewId === "map-view" && state.map) queueMicrotask(() => state.map.resize());
  reportSize();
}

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => selectView(tab.dataset.view));
});

function selectedLayer() {
  return state.layers.find((layer) => layer.layer_id === el("layer-select").value);
}

function renderFeatureWorkspace() {
  const layerSelect = el("layer-select");
  const priorLayer = layerSelect.value;
  layerSelect.replaceChildren(...state.layers.map((layer) => {
    const option = node("option", `${layer.title} · r${layer.revision}`);
    option.value = layer.layer_id;
    return option;
  }));
  if (state.layers.some((layer) => layer.layer_id === priorLayer)) layerSelect.value = priorLayer;
  el("layer-count").textContent = `${state.layers.length}`;
  const layer = selectedLayer();
  el("layer-detail").textContent = layer
    ? `${layer.layer_id} · ${layer.content_class} · schema v${layer.schema.version}`
    : "No layer selected.";
  for (const id of ["publish-layer", "query-layer", "validate-feature", "commit-feature"]) {
    el(id).disabled = !layer;
  }
  el("publication-count").textContent = `${state.publications.length}`;
  replaceList("publication-list", state.publications, (publication) => {
    const item = node("div", undefined, "item");
    item.append(node("strong", publication.title || publication.publication_id));
    item.append(node("div", `${publication.publication_id} · layer r${publication.layer_revision}`, "mono"));
    return item;
  }, "No immutable publications.");
  replaceList("composition-list", state.compositions, (composition) => {
    const item = node("div", undefined, "item");
    item.append(node("strong", composition.title));
    item.append(node("div", `${composition.composition_id} · r${composition.current.revision} · ${composition.current.layers.length} layer(s)`, "mono"));
    return item;
  }, "No map compositions.");
  renderCompositionOptions();
}

function renderCompositionOptions() {
  const select = el("composition-select");
  const prior = select.value;
  select.replaceChildren(...state.compositions.map((composition) => {
    const option = node("option", `${composition.title} · r${composition.current.revision}`);
    option.value = composition.composition_id;
    return option;
  }));
  if (state.compositions.some((composition) => composition.composition_id === prior)) select.value = prior;
  if (!select.value && state.compositions.length) select.value = state.compositions[0].composition_id;
  if (!state.compositions.length) {
    const option = node("option", "No composition available");
    option.value = "";
    select.append(option);
  }
}

function selectedComposition() {
  return state.compositions.find((composition) => composition.composition_id === el("composition-select").value);
}

async function loadFeatureData(reconfigure = false) {
  if (!state.access.feature_read) return;
  [state.layers, state.publications, state.compositions] = await Promise.all([
    read("map://feature-layers"), read("map://publications"), read("map://compositions"),
  ]);
  renderFeatureWorkspace();
  if (state.mapReady && (reconfigure || !state.mapLayers.length)) await configureComposition();
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

async function initializeMap() {
  if (!state.access.feature_read || state.map) return;
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
  state.map = new maplibregl.Map({
    container: "map",
    center: [0, 18], zoom: 1.4, bearing: 0, pitch: 0,
    attributionControl: false,
    style: {
      version: 8,
      sources: {},
      layers: [
        { id: "workspace-background", type: "background", paint: { "background-color": css("--map-water") } },
      ],
    },
  });
  state.map.addControl(new maplibregl.NavigationControl({ visualizePitch: true }), "top-right");
  state.map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");
  await new Promise((resolve, reject) => {
    state.map.once("load", resolve);
    state.map.once("error", (event) => reject(event.error || new Error("MapLibre initialization failed")));
  });
  state.mapReady = true;
  state.map.on("moveend", scheduleViewportRefresh);
  state.map.on("click", showFeaturePopup);
  updateViewportLabel();
  await configureComposition();
}

function css(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function publicationFor(layer) {
  return state.publications.find((publication) =>
    publication.publication_id === layer.publication_id && publication.layer_id === layer.layer_id);
}

async function styleFor(layer) {
  const publication = publicationFor(layer);
  const styleId = layer.style_revision_id || publication && publication.style_revision_id;
  if (!styleId) return null;
  if (!state.styles.has(styleId)) {
    state.styles.set(styleId, await read(`map://feature-style/${styleId}`));
  }
  return state.styles.get(styleId);
}

async function configureComposition() {
  if (!state.mapReady) return;
  const composition = selectedComposition();
  clearRenderedComposition();
  if (!composition) {
    renderMapLayerList();
    setStatus("Create and publish a layer, then add its publication to a composition to view it.", "warn");
    return;
  }
  const view = composition.current.view;
  state.map.jumpTo({
    center: [view.center.longitude_deg, view.center.latitude_deg],
    zoom: view.zoom, bearing: view.bearing_deg, pitch: view.pitch_deg,
  });
  state.mapLayers = await Promise.all(composition.current.layers.map(async (entry, index) => ({
    entry, index, enabled: entry.visible, count: 0, truncated: false,
    layer: state.layers.find((layer) => layer.layer_id === entry.layer_id),
    style: await styleFor(entry),
  })));
  for (const mapLayer of state.mapLayers) installMapLayer(mapLayer);
  renderMapLayerList();
  await refreshViewport();
}

function clearRenderedComposition() {
  state.queryGeneration += 1;
  if (!state.mapReady) return;
  for (const id of state.renderedLayerIds.reverse()) {
    if (state.map.getLayer(id)) state.map.removeLayer(id);
  }
  for (const id of state.sourceIds.reverse()) {
    if (state.map.getSource(id)) state.map.removeSource(id);
  }
  state.renderedLayerIds = [];
  state.sourceIds = [];
  state.mapLayers = [];
}

function installMapLayer(mapLayer) {
  const sourceId = `composition-source-${mapLayer.index}`;
  state.sourceIds.push(sourceId);
  state.map.addSource(sourceId, { type: "geojson", data: { type: "FeatureCollection", features: [] } });
  const rules = mapLayer.style && mapLayer.style.style.rules.length
    ? mapLayer.style.style.rules : [{}];
  rules.forEach((rule, ruleIndex) => {
    const kind = geometryKind(rule.geometry_type);
    if (!kind || kind === "polygon") addVisualLayer(mapLayer, sourceId, rule, ruleIndex, "fill");
    if (!kind || kind === "line" || kind === "polygon") addVisualLayer(mapLayer, sourceId, rule, ruleIndex, "line");
    if (!kind || kind === "point") addVisualLayer(mapLayer, sourceId, rule, ruleIndex, "circle");
  });
}

function geometryKind(value) {
  if (!value) return null;
  if (value === "Point" || value === "MultiPoint") return "point";
  if (value === "LineString" || value === "MultiLineString") return "line";
  if (value === "Polygon" || value === "MultiPolygon") return "polygon";
  return null;
}

function addVisualLayer(mapLayer, sourceId, rule, ruleIndex, type) {
  const id = `composition-${mapLayer.index}-${ruleIndex}-${type}`;
  const color = PALETTE[mapLayer.index % PALETTE.length];
  const opacity = mapLayer.entry.opacity;
  const filterType = type === "fill" ? "Polygon" : type === "circle" ? "Point" : "LineString";
  const paint = type === "fill" ? {
    "fill-color": rule.fill_color || color,
    "fill-opacity": (rule.fill_opacity === undefined ? 0.35 : rule.fill_opacity) * opacity,
  } : type === "line" ? {
    "line-color": rule.line_color || color,
    "line-width": rule.line_width_px || 2,
    "line-opacity": opacity,
  } : {
    "circle-color": rule.circle_color || color,
    "circle-radius": rule.circle_radius_px || 5,
    "circle-opacity": opacity,
    "circle-stroke-color": css("--panel"),
    "circle-stroke-width": 1,
  };
  state.map.addLayer({
    id, source: sourceId, type,
    minzoom: rule.minimum_zoom === undefined ? 0 : rule.minimum_zoom,
    maxzoom: rule.maximum_zoom === undefined ? 24 : rule.maximum_zoom,
    filter: ["==", ["geometry-type"], filterType], paint,
    layout: { visibility: mapLayer.enabled ? "visible" : "none" },
  });
  state.renderedLayerIds.push(id);
}

function renderMapLayerList() {
  replaceList("map-layers", state.mapLayers, (mapLayer) => {
    const row = node("label", undefined, "layer-row");
    const check = document.createElement("input");
    check.type = "checkbox";
    check.checked = mapLayer.enabled;
    check.addEventListener("change", () => {
      mapLayer.enabled = check.checked;
      for (const id of state.renderedLayerIds.filter((candidate) => candidate.startsWith(`composition-${mapLayer.index}-`))) {
        state.map.setLayoutProperty(id, "visibility", mapLayer.enabled ? "visible" : "none");
      }
      scheduleViewportRefresh();
    });
    const copy = node("span", undefined, "layer-title");
    copy.append(node("strong", mapLayer.layer ? mapLayer.layer.title : mapLayer.entry.layer_id));
    copy.append(node("span", `\n${mapLayer.entry.publication_id}`, "mono"));
    row.append(check, copy, node("span", `${mapLayer.count}${mapLayer.truncated ? "+" : ""}`, "count-badge"));
    return row;
  }, "This composition has no publication layers.");
  const total = state.mapLayers.reduce((sum, layer) => sum + layer.count, 0);
  el("map-total").textContent = `${total} feature(s)`;
}

function viewportBoundingBox() {
  const bounds = state.map.getBounds();
  const rawWest = bounds.getWest();
  const rawEast = bounds.getEast();
  const span = rawEast - rawWest;
  if (span >= 359.999) return { west: -180, south: bounds.getSouth(), east: 180, north: bounds.getNorth() };
  const wrap = (value) => ((value + 180) % 360 + 360) % 360 - 180;
  return {
    west: wrap(rawWest), south: Math.max(-90, bounds.getSouth()),
    east: wrap(rawEast), north: Math.min(90, bounds.getNorth()),
  };
}

function updateViewportLabel() {
  if (!state.mapReady) return;
  const bbox = viewportBoundingBox();
  el("viewport").textContent = `${bbox.west.toFixed(4)}, ${bbox.south.toFixed(4)} → ${bbox.east.toFixed(4)}, ${bbox.north.toFixed(4)} · z${state.map.getZoom().toFixed(2)}`;
}

function scheduleViewportRefresh() {
  updateViewportLabel();
  clearTimeout(state.refreshTimer);
  state.refreshTimer = setTimeout(() => { void refreshViewport(); }, 250);
}

async function queryVisibleFeatures(mapLayer, bbox, generation) {
  if (!mapLayer.enabled) return [];
  const features = [];
  let cursor;
  do {
    const request = {
      layer_id: mapLayer.entry.layer_id,
      publication_id: mapLayer.entry.publication_id,
      bbox, limit: 1000,
    };
    if (cursor) request.cursor = cursor;
    const output = await tool("query_features", request);
    if (generation !== state.queryGeneration) return null;
    for (const feature of output.features) {
      features.push({
        type: "Feature", id: feature.id, geometry: feature.geometry,
        properties: {
          ...feature.properties,
          _veoveo_feature_id: feature.id,
          _veoveo_title: feature.title || "",
          _veoveo_semantic_type: feature.featureType,
          _veoveo_layer_id: feature.layer_id,
        },
      });
    }
    cursor = output.next_cursor;
  } while (cursor && features.length < MAX_VIEW_FEATURES_PER_LAYER);
  mapLayer.truncated = Boolean(cursor);
  return features;
}

async function refreshViewport() {
  if (!state.mapReady || !selectedComposition()) return;
  const generation = ++state.queryGeneration;
  const bbox = viewportBoundingBox();
  setStatus("Querying immutable publication pins for the visible map extent…");
  try {
    const results = await Promise.all(state.mapLayers.map((layer) =>
      queryVisibleFeatures(layer, bbox, generation)));
    if (generation !== state.queryGeneration) return;
    const painted = waitForMapPaint();
    results.forEach((features, index) => {
      if (!features) return;
      const mapLayer = state.mapLayers[index];
      mapLayer.count = features.length;
      const source = state.map.getSource(`composition-source-${mapLayer.index}`);
      source.setData({ type: "FeatureCollection", features });
    });
    await painted;
    if (generation !== state.queryGeneration) return;
    const renderedFeatureCount = state.renderedLayerIds.length
      ? state.map.queryRenderedFeatures({ layers: state.renderedLayerIds }).length : 0;
    el("map").dataset.renderedFeatureCount = String(renderedFeatureCount);
    const expectedFeatureCount = results.reduce((sum, features) => sum + (features ? features.length : 0), 0);
    if (expectedFeatureCount && !renderedFeatureCount) {
      throw new Error("MapLibre completed the viewport update without painting any returned feature.");
    }
    renderMapLayerList();
    const truncated = state.mapLayers.some((layer) => layer.truncated);
    setStatus(truncated
      ? `Rendered ${state.mapLayers.reduce((sum, layer) => sum + layer.count, 0)} feature(s); at least one layer reached the ${MAX_VIEW_FEATURES_PER_LAYER}-feature viewport cap.`
      : `Rendered ${state.mapLayers.reduce((sum, layer) => sum + layer.count, 0)} feature(s) from immutable publications.`,
    truncated ? "warn" : "good");
  } catch (error) {
    if (generation === state.queryGeneration) setStatus(`Map query failed: ${error.message}`, "bad");
  }
}

function waitForMapPaint() {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      cleanup();
      reject(new Error("MapLibre did not finish painting the viewport within 5 seconds."));
    }, 5000);
    const idle = () => { cleanup(); resolve(); };
    const failed = (event) => { cleanup(); reject(event.error || new Error("MapLibre viewport paint failed.")); };
    const cleanup = () => {
      clearTimeout(timeout);
      state.map.off("idle", idle);
      state.map.off("error", failed);
    };
    state.map.once("idle", idle);
    state.map.once("error", failed);
  });
}

function showFeaturePopup(event) {
  const features = state.map.queryRenderedFeatures(event.point, { layers: state.renderedLayerIds });
  const feature = features[0];
  if (!feature) return;
  const content = node("div");
  content.append(node("strong", feature.properties._veoveo_title || feature.properties._veoveo_semantic_type || "Map feature"));
  content.append(node("div", feature.properties._veoveo_feature_id, "mono"));
  new maplibregl.Popup({ maxWidth: "320px" }).setLngLat(event.lngLat).setDOMContent(content).addTo(state.map);
}

function resetCompositionView() {
  const composition = selectedComposition();
  if (!composition || !state.map) return;
  const view = composition.current.view;
  state.map.easeTo({ center: [view.center.longitude_deg, view.center.latitude_deg], zoom: view.zoom,
    bearing: view.bearing_deg, pitch: view.pitch_deg, duration: 300 });
}

function renderAdminWorkspace() {
  el("source-count").textContent = `${state.sources.length}`;
  replaceList("source-list", state.sources, (source) => {
    const item = node("div", undefined, "item");
    const head = node("div", undefined, "item-head");
    head.append(node("strong", source.name), node("span", source.enabled ? "active" : "disabled", `chip ${source.enabled ? "on" : ""}`));
    item.append(head, node("div", `${source.source_id} · ${source.adapter_kind} · r${source.record_version}`, "mono"));
    return item;
  }, "No governed sources.");
  const sourceSelect = el("acquire-source");
  sourceSelect.replaceChildren(...state.sources.filter((source) => source.enabled).map((source) => {
    const option = node("option", source.name); option.value = source.source_id; return option;
  }));

  el("acquisition-count").textContent = `${state.acquisitions.length}`;
  renderTable("acquisition-list", ["Acquisition", "Status", "Phase", "Action"], state.acquisitions.map((job) => {
    const action = node("span");
    if (ACTIVE_ACQUISITION.has(job.status) && job.status !== "cancel_requested") {
      const cancel = node("button", "Cancel");
      cancel.addEventListener("click", () => void runAdminAction(() => tool("cancel_acquisition", { acquisition_id: job.acquisition_id })));
      action.append(cancel);
    }
    return [job.acquisition_id, job.status, job.progress && job.progress.phase || "", action];
  }), "No acquisition jobs.");

  const releases = Object.values(state.datasets).flat().sort((left, right) => left.dataset_id.localeCompare(right.dataset_id));
  el("release-count").textContent = `${releases.length}`;
  renderTable("release-list", ["Release", "State", "Version", "Actions"], releases.map((release) => {
    const actions = node("div", undefined, "actions");
    const pointer = state.activeReleases.find((value) => value.dataset_id === release.dataset_id);
    const request = { release_id: release.release_id, expected_record_version: release.record_version,
      expected_active_pointer_version: pointer ? pointer.record_version : 0 };
    const add = (label, name) => {
      const button = node("button", label);
      button.addEventListener("click", () => void runAdminAction(() => tool(name, request)));
      actions.append(button);
    };
    if (release.state === "staged" || release.state === "active") add(release.state === "active" ? "Reconcile" : "Activate", "activate_release");
    if (release.state !== "quarantined" && release.state !== "active") {
      add("Rollback", "rollback_release"); add("Quarantine", "quarantine_release");
    }
    return [release.release_id, release.state, release.version_label, actions];
  }), "No dataset releases.");

  el("profile-count").textContent = `${state.profiles.length}`;
  replaceList("profile-list", state.profiles, (entry) => {
    const metadata = entry.profile && entry.profile.metadata || {};
    const item = node("div", undefined, "item");
    item.append(node("strong", metadata.name || metadata.profile_id || "Mobility profile"));
    item.append(node("div", `${metadata.profile_id || ""} · ${entry.family} · v${metadata.version || ""}`, "mono"));
    return item;
  }, "No mobility profiles.");
  scheduleAdminPoll();
}

function renderTable(hostId, headers, rows, emptyText) {
  const host = el(hostId); host.replaceChildren();
  if (!rows.length) { host.append(node("div", emptyText, "muted item")); return; }
  const table = node("table"), thead = node("thead"), head = node("tr"), tbody = node("tbody");
  headers.forEach((header) => head.append(node("th", header))); thead.append(head);
  rows.forEach((row) => {
    const tr = node("tr");
    row.forEach((value) => { const td = node("td"); td.append(value instanceof Node ? value : document.createTextNode(String(value))); tr.append(td); });
    tbody.append(tr);
  });
  table.append(thead, tbody); host.append(table);
}

async function loadAdminData() {
  if (!state.access.administration) return;
  [state.sources, state.datasets, state.activeReleases, state.acquisitions, state.profiles] = await Promise.all([
    read("map://sources"), read("map://datasets"), read("map://active-releases"),
    read("map://acquisitions"), read("map://mobility-profiles"),
  ]);
  renderAdminWorkspace();
}

function scheduleAdminPoll() {
  clearTimeout(state.pollTimer);
  if (state.acquisitions.some((job) => ACTIVE_ACQUISITION.has(job.status))) {
    state.pollTimer = setTimeout(() => { void loadAdminData().catch((error) => setStatus(error.message, "bad")); }, 5000);
  }
}

async function refreshAll(reconfigure = false) {
  setStatus("Loading the authorized Map workspace…");
  await Promise.all([loadFeatureData(reconfigure), loadAdminData()]);
  setStatus("Map workspace is current.", "good");
  reportSize();
}

async function runFeatureAction(operation, reconfigure = false) {
  try {
    setStatus("Applying governed feature operation…");
    const result = await operation();
    await loadFeatureData(reconfigure);
    setStatus("Governed feature operation completed.", "good");
    return result;
  } catch (error) {
    setStatus(error.message, "bad");
    throw error;
  }
}

async function runAdminAction(operation) {
  try {
    setStatus("Applying governed map data operation…");
    await operation(); await loadAdminData(); setStatus("Governed map data operation completed.", "good");
  } catch (error) { setStatus(error.message, "bad"); }
}

function featureMutationRequest() {
  const layer = selectedLayer();
  if (!layer) throw new Error("Select a feature layer first.");
  return { layer_id: layer.layer_id, expected_layer_revision: layer.revision,
    mutations: [{ action: "create", feature: JSON.parse(el("feature-json").value) }] };
}

el("refresh").addEventListener("click", () => void refreshAll(true).catch((error) => setStatus(error.message, "bad")));
el("reload-map").addEventListener("click", () => void refreshViewport());
el("fit-composition").addEventListener("click", resetCompositionView);
el("composition-select").addEventListener("change", () => void configureComposition().catch((error) => setStatus(error.message, "bad")));
el("layer-select").addEventListener("change", renderFeatureWorkspace);
el("layer-form").addEventListener("submit", (event) => {
  event.preventDefault();
  void runFeatureAction(() => tool("create_feature_layer", {
    title: el("layer-title").value, content_class: el("content-class").value,
    property_schema: JSON.parse(el("schema-json").value),
  }), true).catch(() => {});
});
el("validate-feature").addEventListener("click", () => void runFeatureAction(async () => {
  const output = await tool("validate_feature_changes", featureMutationRequest());
  el("validation-output").textContent = JSON.stringify(output, null, 2);
  return output;
}).catch(() => {}));
el("commit-feature").addEventListener("click", () => void runFeatureAction(async () => {
  const request = featureMutationRequest(); request.idempotency_key = uuid();
  const output = await tool("commit_feature_changes", request);
  el("validation-output").textContent = JSON.stringify(output, null, 2);
  return output;
}, true).catch(() => {}));
el("query-layer").addEventListener("click", () => void runFeatureAction(async () => {
  const layer = selectedLayer();
  const output = await tool("query_features", { layer_id: layer.layer_id, limit: 100 });
  el("features-output").textContent = JSON.stringify(output, null, 2);
  return output;
}).catch(() => {}));
el("publish-layer").addEventListener("click", () => void runFeatureAction(() => {
  const layer = selectedLayer();
  return tool("publish_feature_layer", { layer_id: layer.layer_id, expected_layer_revision: layer.revision });
}, true).catch(() => {}));
el("create-composition").addEventListener("click", () => void runFeatureAction(
  () => tool("create_map_composition", JSON.parse(el("composition-json").value)), true).catch(() => {}));

el("source-form").addEventListener("submit", (event) => {
  event.preventDefault();
  void runAdminAction(() => tool("register_source", {
    source: JSON.parse(el("source-json").value), idempotency_key: uuid(),
  }));
});
el("profile-form").addEventListener("submit", (event) => {
  event.preventDefault();
  void runAdminAction(() => tool("register_mobility_profile", {
    profile: JSON.parse(el("profile-json").value), idempotency_key: uuid(),
  }));
});

function requestedCoverage() {
  const fields = [["west", "bbox-west", -180, 180], ["south", "bbox-south", -90, 90],
    ["east", "bbox-east", -180, 180], ["north", "bbox-north", -90, 90]];
  const coverage = {};
  for (const [name, id, minimum, maximum] of fields) {
    const raw = el(id).value.trim();
    if (!raw) throw new Error("Enter west, south, east, and north coverage bounds before starting.");
    const value = Number(raw);
    if (!Number.isFinite(value) || value < minimum || value > maximum) throw new Error(`${name} must be between ${minimum} and ${maximum}.`);
    coverage[name] = value;
  }
  if (coverage.south > coverage.north) throw new Error("South coverage must not exceed north coverage.");
  return coverage;
}

el("acquire-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const submit = el("acquire-submit");
  try {
    const sourceId = el("acquire-source").value;
    if (!sourceId) throw new Error("Select an enabled source before starting.");
    const coverage = requestedCoverage();
    submit.disabled = true; submit.textContent = "Starting…";
    void runAdminAction(() => tool("start_acquisition", {
      source_id: sourceId, requested_coverage: coverage, idempotency_key: uuid(),
    })).finally(() => { submit.disabled = false; submit.textContent = "Start acquisition"; });
  } catch (error) { setStatus(error.message, "bad"); }
});

function applyHostContext(context) {
  if (context && (context.theme === "dark" || context.theme === "light")) {
    document.documentElement.dataset.theme = context.theme;
  }
}

function reportSize() {
  bridge.notify("ui/notifications/size-changed", {
    height: Math.ceil(document.documentElement.getBoundingClientRect().height) + 8,
  });
}

bridge.on("ui/notifications/host-context-changed", (params) => applyHostContext(params && (params.hostContext || params)));
bridge.on("ui/notifications/tool-result", () => void refreshAll(true).catch((error) => setStatus(error.message, "bad")));
bridge.on("ui/notifications/resource-updated", (params) => {
  const uri = params && params.uri || params;
  if (typeof uri === "string" && uri.startsWith("map://")) {
    void refreshAll(true).catch((error) => setStatus(error.message, "bad"));
  }
});
bridge.on("ui/resource-teardown", (_params, id) => {
  state.closing = true;
  clearTimeout(state.refreshTimer); clearTimeout(state.pollTimer);
  state.map?.remove();
  if (id !== undefined) bridge.post({ jsonrpc: "2.0", id, result: {} });
});

(async () => {
  try {
    const initialized = await bridge.request("ui/initialize", {
      protocolVersion: "2026-01-26",
      appInfo: { name: "map-workspace", version: "1.0.0" },
      appCapabilities: { availableDisplayModes: ["inline"] },
    });
    bridge.notify("ui/notifications/initialized", {});
    applyHostContext(initialized && initialized.hostContext);
    state.access = await read("map://workspace");
    renderAccess();
    await refreshAll();
    if (state.access.feature_read) {
      await initializeMap();
      void bridge.request("subscriptions/listen", { notifications: { resourceSubscriptions: [
        "map://feature-layers", "map://publications", "map://compositions",
      ] } }).catch((error) => { if (!state.closing) setStatus(`Live map updates unavailable: ${error.message}`, "warn"); });
    }
  } catch (error) {
    setStatus(`Workspace initialization failed: ${error.message}`, "bad");
  }
})();
