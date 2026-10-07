import {element as el,input} from "./dom.js";
import {resourceEnvelope, resourceJson, structuredResult, toolEnvelope, taskEnvelope} from "../../../mcp/apps-extension/browser/admission.js";
import {admit, resourceValue, toolValue} from "./contracts.js";
"use strict";

if(typeof VENDOR==="undefined"||!VENDOR.THREE||typeof VENDOR.OrbitControls!=="function")throw new Error("View graphics vendor is unavailable");
for(const name of ["BufferAttribute", "BufferGeometry", "CameraHelper", "CanvasTexture", "Color", "DirectionalLight", "Group", "HemisphereLight", "Matrix4", "Mesh", "MeshBasicMaterial", "MeshLambertMaterial", "PerspectiveCamera", "Quaternion", "Raycaster", "Scene", "SphereGeometry", "Sprite", "SpriteMaterial", "Texture", "Vector2", "Vector3", "WebGLRenderer"])if(typeof VENDOR.THREE[name]!=="function")throw new Error(`View graphics vendor lacks ${name}`);
if(typeof VENDOR.THREE.RepeatWrapping!=="number"||typeof VENDOR.THREE.ClampToEdgeWrapping!=="number"||typeof VENDOR.THREE.MirroredRepeatWrapping!=="number"||typeof VENDOR.THREE.DoubleSide!=="number"||typeof VENDOR.THREE.FrontSide!=="number"||typeof VENDOR.THREE.SRGBColorSpace!=="string")throw new Error("View graphics constants are unavailable");
const THREE = VENDOR.THREE;
const OrbitControls = VENDOR.OrbitControls;

/* ---------------------------------------------------------------- bridge --
 * View side of the MCP Apps (ext-apps 2026-01-26) postMessage protocol.
 * The host is the only counterparty; the frame runs in an opaque origin, so
 * postMessage targets "*" and inbound messages are trusted only for shape.
 */
const bridge = (() => {
  let nextId = 1;
  const pending = new Map();
  const notificationHandlers = new Map();

  function post(message) {
    window.parent.postMessage(message, "*");
  }
  function request(method, params) {
    return new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      post({ jsonrpc: "2.0", id, method, params });
    });
  }
  window.addEventListener("message", (event) => {
    if (event.source !== parent) return;
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
    if (message.method) {
      const handler = notificationHandlers.get(message.method);
      if (handler) handler(message.params, message.id);
      else if (message.id !== undefined) {
        post({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "method not found" } });
      }
    }
  });
  return {
    request,
    notify: (method, params) => post({ jsonrpc: "2.0", method, params }),
    on: (method, handler) => notificationHandlers.set(method, handler),
    post,
  };
})();

/* ------------------------------------------------------------- transport -- */
/** @template {keyof import("./generated/view").AppContracts} K @param {string} uri @param {K} root @returns {Promise<import("./generated/view").AppContracts[K]>} */
async function readJsonResource(uri,root) {
  return resourceValue(uri, resourceJson(await bridge.request("resources/read", {uri}), uri),root);
}

async function readBlobResource(uri) {
  const result = resourceEnvelope(await bridge.request("resources/read", { uri }));
  const contents = (result && result.contents) || [];
  const item = contents.find(item=>item.uri===uri && "blob" in item);
  const blob = item && "blob" in item ? item.blob : undefined;
  if (typeof blob !== "string") {
    console.error(`resource ${uri} returned no blob`);
    throw new Error("The View server returned data this app couldn't read. Reload to try again.");
  }
  const binary = atob(blob);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

function toolFailureText(result, name) {
  const content = Array.isArray(result && result.content) ? result.content : [];
  const text = content.find((entry) => entry && entry.type === "text" && typeof entry.text === "string");
  if (text) return text.text;
  console.error(`MCP tool ${name} failed`, result);
  return "The View server didn't complete that request. Try again.";
}

/* Full CallToolResult (content blocks included); throws on isError. */
async function callToolRaw(name, args) {
  const params = { name: projectedToolName(name), arguments: args };
  const result = toolEnvelope(await bridge.request("tools/call", params));
  if (result && result.isError) {
    throw new Error(toolFailureText(result, name));
  }
  return result;
}

/** @template {keyof typeof import("./contracts.js").tools} N @param {N} name @param {Record<string,unknown>} args @returns {Promise<import("./generated/view").AppContracts[(typeof import("./contracts.js").tools)[N]]>} */
async function callTool(name, args) {
  const result = await callToolRaw(name, args);
  return toolValue(name, structuredResult(result), args);
}

const tasks = {
  get: async (taskId) => taskEnvelope(await bridge.request("tasks/get", { taskId }),taskId),
  cancel: (taskId) => bridge.request("tasks/cancel", { taskId }),
};

/* --------------------------------------------------------------------- m4 --
 * Column-major 4x4 math on Float64Array. All planetary-scale composition
 * happens here; three.js (f32) only ever sees scene-local results.
 */
const m4 = (() => {
  function identity() {
    const m = new Float64Array(16);
    m[0] = m[5] = m[10] = m[15] = 1;
    return m;
  }
  function fromArray(values) {
    return Float64Array.from(values);
  }
  function mul(a, b) {
    const out = new Float64Array(16);
    for (let c = 0; c < 4; c++) {
      for (let r = 0; r < 4; r++) {
        out[c * 4 + r] =
          a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
      }
    }
    return out;
  }
  function translation(v) {
    const m = identity();
    m[12] = v[0];
    m[13] = v[1];
    m[14] = v[2];
    return m;
  }
  function transformPoint(m, p) {
    return [
      m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
      m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
      m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ];
  }
  /* Inverse of a rotation+translation matrix. */
  function invertRigid(m) {
    const out = identity();
    out[0] = m[0]; out[1] = m[4]; out[2] = m[8];
    out[4] = m[1]; out[5] = m[5]; out[6] = m[9];
    out[8] = m[2]; out[9] = m[6]; out[10] = m[10];
    out[12] = -(out[0] * m[12] + out[4] * m[13] + out[8] * m[14]);
    out[13] = -(out[1] * m[12] + out[5] * m[13] + out[9] * m[14]);
    out[14] = -(out[2] * m[12] + out[6] * m[13] + out[10] * m[14]);
    return out;
  }
  function toThree(m) {
    return new THREE.Matrix4().fromArray(m);
  }
  return { identity, fromArray, mul, translation, transformPoint, invertRigid, toThree };
})();

/* -------------------------------------------------------------------- geo --
 * Line-for-line port of the server's geodesy.rs so the frustum gizmo matches
 * what the renderer produces. Positions are {latitudeDegrees,
 * longitudeDegrees, ellipsoidalHeightMeters}; vectors are [x, y, z].
 */
const geo = (() => {
  const A = 6378137.0;
  const F = 1 / 298.257223563;
  const E2 = F * (2 - F);
  const RAD = Math.PI / 180;

  const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const cross = (a, b) => [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
  const scale = (v, s) => [v[0] * s, v[1] * s, v[2] * s];
  const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
  const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
  const length = (v) => Math.hypot(v[0], v[1], v[2]);
  function normalizeOr(v, fallback) {
    const len = length(v);
    return len > 1e-12 ? scale(v, 1 / len) : fallback;
  }
  const normalize = (v) => normalizeOr(v, [1, 0, 0]);
  function rotateAround(v, axis, angle) {
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    const k = axis;
    return add(
      add(scale(v, cos), scale(cross(k, v), sin)),
      scale(k, dot(k, v) * (1 - cos))
    );
  }

  function geodeticToEcef(p) {
    const lat = p.latitudeDegrees * RAD;
    const lon = p.longitudeDegrees * RAD;
    const sinLat = Math.sin(lat), cosLat = Math.cos(lat);
    const sinLon = Math.sin(lon), cosLon = Math.cos(lon);
    const n = A / Math.sqrt(1 - E2 * sinLat * sinLat);
    const h = p.ellipsoidalHeightMeters;
    return [(n + h) * cosLat * cosLon, (n + h) * cosLat * sinLon, (n * (1 - E2) + h) * sinLat];
  }

  function ecefToGeodetic(ecef) {
    const b = A * (1 - F);
    const epSq = (A * A - b * b) / (b * b);
    const p = Math.hypot(ecef[0], ecef[1]);
    const theta = Math.atan2(ecef[2] * A, p * b);
    const lon = Math.atan2(ecef[1], ecef[0]);
    const lat = Math.atan2(
      ecef[2] + epSq * b * Math.sin(theta) ** 3,
      p - E2 * A * Math.cos(theta) ** 3
    );
    const sinLat = Math.sin(lat);
    const n = A / Math.sqrt(1 - E2 * sinLat * sinLat);
    return {
      latitudeDegrees: lat / RAD,
      longitudeDegrees: lon / RAD,
      ellipsoidalHeightMeters: p / Math.cos(lat) - n,
    };
  }

  function enuBasis(p) {
    const lat = p.latitudeDegrees * RAD;
    const lon = p.longitudeDegrees * RAD;
    const sinLat = Math.sin(lat), cosLat = Math.cos(lat);
    const sinLon = Math.sin(lon), cosLon = Math.cos(lon);
    return {
      east: [-sinLon, cosLon, 0],
      north: [-sinLat * cosLon, -sinLat * sinLon, cosLat],
      up: [cosLat * cosLon, cosLat * sinLon, sinLat],
    };
  }

  /* Local frame at origin: +X east, +Y up, -Z north. */
  function worldFromEcef(origin) {
    const { east, north, up } = enuBasis(origin);
    const o = geodeticToEcef(origin);
    const rows = [east, up, [-north[0], -north[1], -north[2]]];
    return Float64Array.from([
      rows[0][0], rows[1][0], rows[2][0], 0,
      rows[0][1], rows[1][1], rows[2][1], 0,
      rows[0][2], rows[1][2], rows[2][2], 0,
      -dot(rows[0], o), -dot(rows[1], o), -dot(rows[2], o), 1,
    ]);
  }

  function validatePosition(p, label) {
    if (!Number.isFinite(p.latitudeDegrees) || p.latitudeDegrees < -90 || p.latitudeDegrees > 90) {
      throw new Error(`${label} latitude must be between -90 and 90 degrees`);
    }
    if (!Number.isFinite(p.longitudeDegrees) || p.longitudeDegrees < -180 || p.longitudeDegrees > 180) {
      throw new Error(`${label} longitude must be between -180 and 180 degrees`);
    }
    if (!Number.isFinite(p.ellipsoidalHeightMeters) || p.ellipsoidalHeightMeters < -20000 || p.ellipsoidalHeightMeters > 100000000) {
      throw new Error(`${label} altitude is outside the supported range`);
    }
  }
  function validateFov(value) {
    if (!Number.isFinite(value) || value < 1 || value > 160) {
      throw new Error("vertical FOV must be between 1 and 160 degrees");
    }
  }

  function orientationToward(eye, target, rollDegrees) {
    const delta = sub(geodeticToEcef(target), geodeticToEcef(eye));
    const { east, north, up } = enuBasis(eye);
    const local = [dot(delta, east), dot(delta, north), dot(delta, up)];
    if (dot(local, local) < 1e-12) throw new Error("camera eye and target must differ");
    const horizontal = Math.hypot(local[0], local[1]);
    const heading = ((Math.atan2(local[0], local[1]) / RAD) % 360 + 360) % 360;
    return {
      headingDegrees: heading,
      pitchDegrees: Math.atan2(local[2], horizontal) / RAD,
      rollDegrees: rollDegrees,
    };
  }

  /* Mirrors geodesy.rs resolve_camera plus contract.rs validation ranges. */
  function resolveCamera(definition) {
    if (definition.kind === "pose") {
      validatePosition(definition.position, "camera");
      validateFov(definition.verticalFovDegrees);
      const o = definition.orientation;
      if (![o.headingDegrees, o.pitchDegrees, o.rollDegrees].every(Number.isFinite) ||
          o.pitchDegrees < -90 || o.pitchDegrees > 90) {
        throw new Error("pitch must be between -90 and 90 degrees");
      }
      return {
        position: { ...definition.position },
        orientation: { ...definition.orientation },
        verticalFovDegrees: definition.verticalFovDegrees,
      };
    }
    if (definition.kind === "look_at") {
      validatePosition(definition.eye, "eye");
      validatePosition(definition.target, "target");
      validateFov(definition.verticalFovDegrees);
      return {
        position: { ...definition.eye },
        orientation: orientationToward(definition.eye, definition.target, 0),
        verticalFovDegrees: definition.verticalFovDegrees,
      };
    }
    if (definition.kind === "orbit_target") {
      validatePosition(definition.target, "target");
      validateFov(definition.verticalFovDegrees);
      if (!Number.isFinite(definition.distanceMeters) || definition.distanceMeters < 0.1 || definition.distanceMeters > 100000000) {
        throw new Error("orbit distance must be between 0.1 m and 100,000 km");
      }
      if (!Number.isFinite(definition.elevationDegrees) || definition.elevationDegrees < -89.9 || definition.elevationDegrees > 89.9) {
        throw new Error("orbit elevation must be between -89.9 and 89.9 degrees");
      }
      const targetEcef = geodeticToEcef(definition.target);
      const { east, north, up } = enuBasis(definition.target);
      const azimuth = definition.azimuthDegrees * RAD;
      const elevation = definition.elevationDegrees * RAD;
      const horizontal = definition.distanceMeters * Math.cos(elevation);
      const offset = add(
        add(scale(east, horizontal * Math.sin(azimuth)), scale(north, horizontal * Math.cos(azimuth))),
        scale(up, definition.distanceMeters * Math.sin(elevation))
      );
      const eye = ecefToGeodetic(add(targetEcef, offset));
      return {
        position: eye,
        orientation: orientationToward(eye, definition.target, 0),
        verticalFovDegrees: definition.verticalFovDegrees,
      };
    }
    throw new Error(`unknown camera kind ${definition.kind}`);
  }

  /* Columns (right, up, -forward, position): exactly a three.js camera
   * world matrix in the local frame. Mirrors geodesy.rs camera_world_transform. */
  function cameraWorldTransform(pose, origin) {
    const world = worldFromEcef(origin);
    const position = m4.transformPoint(world, geodeticToEcef(pose.position));
    const h = pose.orientation.headingDegrees * RAD;
    const p = pose.orientation.pitchDegrees * RAD;
    const r = pose.orientation.rollDegrees * RAD;
    const forward = normalize([Math.sin(h) * Math.cos(p), Math.sin(p), -Math.cos(h) * Math.cos(p)]);
    let right = normalizeOr(cross(forward, [0, 1, 0]), [1, 0, 0]);
    let up = normalize(cross(right, forward));
    if (r !== 0) {
      right = rotateAround(right, forward, r);
      up = rotateAround(up, forward, r);
    }
    return Float64Array.from([
      right[0], right[1], right[2], 0,
      up[0], up[1], up[2], 0,
      -forward[0], -forward[1], -forward[2], 0,
      position[0], position[1], position[2], 1,
    ]);
  }

  return { geodeticToEcef, ecefToGeodetic, enuBasis, worldFromEcef, orientationToward, resolveCamera, cameraWorldTransform };
})();

/* ------------------------------------------------------------------ draco --
 * Main-thread decode through the vendored Emscripten JS decoder. No workers,
 * no wasm, no fetches: the frame CSP allows none of them.
 */
const draco = (() => {
  let modulePromise = /** @type {Promise<import("draco3d").DecoderModule>|null} */ (null);
  function init() {
    if (!modulePromise) {
      modulePromise = new Promise((resolve, reject) => {
        try {
          if(typeof DracoDecoderModule!=="function")throw new Error("View Draco decoder is unavailable");
          DracoDecoderModule({ onModuleLoaded: resolve });
        } catch (cause) {
          reject(cause);
        }
      });
    }
    return modulePromise;
  }

  /** @param {import("draco3d").DecoderModule} mod */
  function heapInfo(mod, componentType) {
    switch (componentType) {
      case 5126: return { dt: mod.DT_FLOAT32, Ctor: Float32Array, heap: () => mod.HEAPF32 };
      case 5125: return { dt: mod.DT_UINT32, Ctor: Uint32Array, heap: () => mod.HEAPU32 };
      case 5123: return { dt: mod.DT_UINT16, Ctor: Uint16Array, heap: () => mod.HEAPU16 };
      case 5121: return { dt: mod.DT_UINT8, Ctor: Uint8Array, heap: () => mod.HEAPU8 };
      case 5122: return { dt: mod.DT_INT16, Ctor: Int16Array, heap: () => mod.HEAP16 };
      case 5120: return { dt: mod.DT_INT8, Ctor: Int8Array, heap: () => mod.HEAP8 };
      default: throw new Error(`unsupported accessor component type ${componentType}`);
    }
  }

  /* attributeIds: KHR_draco_mesh_compression.attributes (name → unique id);
   * accessorTypes: name → {componentType, normalized} from the primitive's
   * glTF accessors, which stay authoritative for layout. */
  /** @param {import("draco3d").DecoderModule} mod */
  function decodeMesh(mod, bytes, attributeIds, accessorTypes) {
    const buffer = new mod.DecoderBuffer();
    buffer.Init(new Int8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength), bytes.byteLength);
    const decoder = new mod.Decoder();
    const mesh = new mod.Mesh();
    try {
      const status = decoder.DecodeBufferToMesh(buffer, mesh);
      if (!status.ok() || mesh.ptr === 0) {
        throw new Error(`draco decode failed: ${status.error_msg()}`);
      }
      const points = mesh.num_points();
      const attributes = {};
      for (const [name, uniqueId] of Object.entries(attributeIds)) {
        const accessor = accessorTypes[name] || { componentType: 5126, normalized: false };
        const info = heapInfo(mod, accessor.componentType);
        const attribute = decoder.GetAttributeByUniqueId(mesh, uniqueId);
        const components = attribute.num_components();
        const count = points * components;
        const byteLength = count * info.Ctor.BYTES_PER_ELEMENT;
        const ptr = mod._malloc(byteLength);
        try {
          decoder.GetAttributeDataArrayForAllPoints(mesh, attribute, info.dt, byteLength, ptr);
          attributes[name] = {
            array: info.heap().slice(ptr / info.Ctor.BYTES_PER_ELEMENT, ptr / info.Ctor.BYTES_PER_ELEMENT + count),
            components,
            normalized: Boolean(accessor.normalized),
          };
        } finally {
          mod._free(ptr);
        }
      }
      const faces = mesh.num_faces();
      const indexBytes = faces * 3 * 4;
      const indexPtr = mod._malloc(indexBytes);
      let index;
      try {
        decoder.GetTrianglesUInt32Array(mesh, indexBytes, indexPtr);
        index = new Uint32Array(mod.HEAPU32.buffer, indexPtr, faces * 3).slice();
      } finally {
        mod._free(indexPtr);
      }
      return { attributes, index };
    } finally {
      mod.destroy(mesh);
      mod.destroy(decoder);
      mod.destroy(buffer);
    }
  }

  return { init, decodeMesh };
})();

/* -------------------------------------------------------------------- glb --
 * Narrow GLB 2.0 parser mirroring the server's decode.rs: draco primitives
 * (plus plain accessors for local fixtures), CESIUM_RTC or planetary root
 * translation extracted in f64, base-color materials, embedded textures.
 */
const glb = (() => {
  const PLANETARY_TRANSLATION_M = 2000000;
  const COMPONENT_ARRAYS = {
    5120: Int8Array, 5121: Uint8Array, 5122: Int16Array,
    5123: Uint16Array, 5125: Uint32Array, 5126: Float32Array,
  };
  const TYPE_COMPONENTS = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };
  /** @type {Record<number,"ClampToEdgeWrapping"|"MirroredRepeatWrapping"|"RepeatWrapping">} */
  const WRAP_MODES = { 33071: "ClampToEdgeWrapping", 33648: "MirroredRepeatWrapping", 10497: "RepeatWrapping" };

  function parse(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (bytes.byteLength < 20 || view.getUint32(0, true) !== 0x46546c67) {
      throw new Error("content is not a GLB 2.0 file");
    }
    if (view.getUint32(4, true) !== 2) throw new Error("unsupported GLB version");
    const declared = Math.min(view.getUint32(8, true), bytes.byteLength);
    let offset = 12;
    let json = null;
    let bin = new Uint8Array(0);
    while (offset + 8 <= declared) {
      const length = view.getUint32(offset, true);
      const kind = view.getUint32(offset + 4, true);
      offset += 8;
      const end = offset + length;
      if (end > declared) throw new Error("GLB is truncated");
      if (kind === 0x4e4f534a) {
        json = JSON.parse(new TextDecoder().decode(bytes.subarray(offset, end)));
      } else if (kind === 0x004e4942) {
        bin = bytes.subarray(offset, end);
      }
      offset = end;
    }
    if (!json) throw new Error("GLB JSON chunk is missing");
    return { json, bin };
  }

  function bufferViewBytes(json, bin, index) {
    const view = json.bufferViews[index];
    const offset = view.byteOffset || 0;
    return bin.subarray(offset, offset + view.byteLength);
  }

  function accessorArray(json, bin, index) {
    const accessor = json.accessors[index];
    const Ctor = COMPONENT_ARRAYS[accessor.componentType];
    const components = TYPE_COMPONENTS[accessor.type];
    if (!Ctor || !components) throw new Error("unsupported accessor layout");
    const view = json.bufferViews[accessor.bufferView];
    const start = (view.byteOffset || 0) + (accessor.byteOffset || 0);
    const elementBytes = Ctor.BYTES_PER_ELEMENT * components;
    const stride = view.byteStride || elementBytes;
    if (stride === elementBytes) {
      const copy = bin.slice(start, start + accessor.count * elementBytes);
      return {
        array: new Ctor(copy.buffer, 0, accessor.count * components),
        components,
        normalized: Boolean(accessor.normalized),
      };
    }
    const out = new Ctor(accessor.count * components);
    for (let element = 0; element < accessor.count; element++) {
      const base = start + element * stride;
      const chunk = bin.slice(base, base + elementBytes);
      out.set(new Ctor(chunk.buffer, 0, components), element * components);
    }
    return { array: out, components, normalized: Boolean(accessor.normalized) };
  }

  function nodeTranslation(node) {
    if (!node) return null;
    if (Array.isArray(node.matrix) && node.matrix.length === 16) {
      return [node.matrix[12], node.matrix[13], node.matrix[14]];
    }
    if (Array.isArray(node.translation) && node.translation.length === 3) {
      return node.translation.slice();
    }
    return null;
  }

  function subtractTranslation(node, center) {
    if (!node) return;
    if (Array.isArray(node.matrix) && node.matrix.length === 16) {
      node.matrix[12] -= center[0];
      node.matrix[13] -= center[1];
      node.matrix[14] -= center[2];
    } else if (Array.isArray(node.translation) && node.translation.length === 3) {
      node.translation[0] -= center[0];
      node.translation[1] -= center[1];
      node.translation[2] -= center[2];
    }
  }

  function sceneRootIndices(json) {
    const sceneIndex = Number.isInteger(json.scene) ? json.scene : 0;
    const scene = (json.scenes || [])[sceneIndex] || {};
    return Array.isArray(scene.nodes) ? scene.nodes : [];
  }

  /* Mirrors decode.rs prepare_glb: explicit CESIUM_RTC center, else a
   * planetary-magnitude scene-root translation is hoisted out (in f64, on
   * the JSON values, before any f32 matrix exists). */
  function extractRtc(json) {
    const explicit = json.extensions && json.extensions.CESIUM_RTC && json.extensions.CESIUM_RTC.center;
    if (Array.isArray(explicit) && explicit.length === 3 && explicit.every(Number.isFinite)) {
      return explicit.slice();
    }
    const roots = sceneRootIndices(json);
    const nodes = json.nodes || [];
    let center = null;
    for (const index of roots) {
      const translation = nodeTranslation(nodes[index]);
      if (translation && Math.hypot(translation[0], translation[1], translation[2]) > PLANETARY_TRANSLATION_M) {
        center = translation;
        break;
      }
    }
    if (!center) return null;
    for (const index of roots) subtractTranslation(nodes[index], center);
    return center;
  }

  function nodeMatrixF64(node) {
    if (Array.isArray(node.matrix) && node.matrix.length === 16) {
      return m4.fromArray(node.matrix);
    }
    const t = node.translation || [0, 0, 0];
    const r = node.rotation || [0, 0, 0, 1];
    const s = node.scale || [1, 1, 1];
    const three = new THREE.Matrix4().compose(
      new THREE.Vector3(t[0], t[1], t[2]),
      new THREE.Quaternion(r[0], r[1], r[2], r[3]),
      new THREE.Vector3(s[0], s[1], s[2])
    );
    const out = m4.fromArray(three.elements);
    /* Preserve the f64 translation the f32 compose truncated. */
    out[12] = t[0];
    out[13] = t[1];
    out[14] = t[2];
    return out;
  }

  /* Builds a THREE.Group of meshes whose matrices are the glTF node chains
   * (small after RTC extraction, f32-safe). The caller owns the group-level
   * local←content transform. Returns { group, rtcCenter, attribution }. */
  /** @param {import("draco3d").DecoderModule} mod */
  async function build(mod, parsed) {
    const { json, bin } = parsed;
    const rtcCenter = extractRtc(json);
    const attribution = ((json.asset || {}).copyright || "")
      .split(";").map((line) => line.trim()).filter(Boolean);
    const group = new THREE.Group();
    group.matrixAutoUpdate = false;
    const texturePromises = new Map();

    async function loadTexture(textureIndex) {
      if (texturePromises.has(textureIndex)) return texturePromises.get(textureIndex);
      const promise = (async () => {
        const textureDef = (json.textures || [])[textureIndex];
        if (!textureDef || textureDef.source === undefined) return null;
        const image = (json.images || [])[textureDef.source];
        if (!image || image.bufferView === undefined) return null;
        const bytes = bufferViewBytes(json, bin, image.bufferView);
        const bitmap = await createImageBitmap(
          new Blob([bytes.slice()], { type: image.mimeType || "image/jpeg" })
        );
        const texture = new THREE.Texture(bitmap);
        texture.flipY = false;
        texture.colorSpace = THREE.SRGBColorSpace;
        const sampler = (json.samplers || [])[textureDef.sampler] || {};
        texture.wrapS = THREE[WRAP_MODES[sampler.wrapS] || "RepeatWrapping"];
        texture.wrapT = THREE[WRAP_MODES[sampler.wrapT] || "RepeatWrapping"];
        texture.needsUpdate = true;
        return texture;
      })().catch(() => null);
      texturePromises.set(textureIndex, promise);
      return promise;
    }

    async function buildMaterial(index, hasVertexColors) {
      const material = index === undefined ? {} : (json.materials || [])[index] || {};
      const pbr = material.pbrMetallicRoughness || {};
      const factor = pbr.baseColorFactor || [1, 1, 1, 1];
      const unlit = Boolean(material.extensions && material.extensions.KHR_materials_unlit);
      const params = {
        color: new THREE.Color(factor[0], factor[1], factor[2]),
        side: material.doubleSided ? THREE.DoubleSide : THREE.FrontSide,
      };
      const output = unlit ? new THREE.MeshBasicMaterial(params) : new THREE.MeshLambertMaterial(params);
      if (material.alphaMode === "BLEND") {
        output.transparent = true;
        output.opacity = factor[3];
      } else if (material.alphaMode === "MASK") {
        output.alphaTest = material.alphaCutoff === undefined ? 0.5 : material.alphaCutoff;
      }
      if (hasVertexColors) output.vertexColors = true;
      if (pbr.baseColorTexture) {
        const texture = await loadTexture(pbr.baseColorTexture.index);
        if (texture) output.map = texture;
      }
      output.needsUpdate = true;
      return output;
    }

    async function buildPrimitive(primitive, nodeF64) {
      if (primitive.mode !== undefined && primitive.mode !== 4) return null;
      const dracoExt = primitive.extensions && primitive.extensions.KHR_draco_mesh_compression;
      let decoded;
      if (dracoExt) {
        const accessorTypes = {};
        for (const name of Object.keys(dracoExt.attributes || {})) {
          const accessorIndex = (primitive.attributes || {})[name];
          const accessor = accessorIndex === undefined ? null : json.accessors[accessorIndex];
          accessorTypes[name] = accessor || { componentType: 5126, normalized: false };
        }
        decoded = draco.decodeMesh(
          mod,
          bufferViewBytes(json, bin, dracoExt.bufferView),
          dracoExt.attributes || {},
          accessorTypes
        );
      } else {
        decoded = { attributes: {}, index: null };
        for (const [name, accessorIndex] of Object.entries(primitive.attributes || {})) {
          decoded.attributes[name] = accessorArray(json, bin, accessorIndex);
        }
        if (primitive.indices !== undefined) {
          decoded.index = accessorArray(json, bin, primitive.indices).array;
        }
      }
      const position = decoded.attributes.POSITION;
      if (!position) return null;
      const geometry = new THREE.BufferGeometry();
      const setAttr = (three, source) => geometry.setAttribute(
        three,
        new THREE.BufferAttribute(source.array, source.components, source.normalized)
      );
      setAttr("position", position);
      if (decoded.attributes.NORMAL) setAttr("normal", decoded.attributes.NORMAL);
      if (decoded.attributes.TEXCOORD_0) setAttr("uv", decoded.attributes.TEXCOORD_0);
      if (decoded.attributes.COLOR_0) setAttr("color", decoded.attributes.COLOR_0);
      if (decoded.index) {
        geometry.setIndex(new THREE.BufferAttribute(decoded.index, 1));
      }
      if (!decoded.attributes.NORMAL) geometry.computeVertexNormals();
      const material = await buildMaterial(primitive.material, Boolean(decoded.attributes.COLOR_0));
      const mesh = new THREE.Mesh(geometry, material);
      mesh.matrixAutoUpdate = false;
      mesh.matrix.fromArray(nodeF64);
      return mesh;
    }

    async function buildNode(index, parentF64) {
      const node = (json.nodes || [])[index];
      if (!node) return;
      const transform = m4.mul(parentF64, nodeMatrixF64(node));
      if (node.mesh !== undefined) {
        const mesh = (json.meshes || [])[node.mesh];
        for (const primitive of (mesh && mesh.primitives) || []) {
          const built = await buildPrimitive(primitive, transform);
          if (built) group.add(built);
        }
      }
      for (const child of node.children || []) await buildNode(child, transform);
    }

    for (const index of sceneRootIndices(json)) await buildNode(index, m4.identity());
    return { group, rtcCenter, attribution };
  }

  return { parse, build };
})();

/* ------------------------------------------------------------------ scene -- */
class SceneView {
  constructor(canvas) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(55, 1, 0.5, 200000);
    this.camera.position.set(500, 400, 500);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.maxDistance = 60000;

    this.tiles = new THREE.Group();
    this.scene.add(this.tiles);
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x5c584f, 2.6));
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(600, 900, 400);
    this.scene.add(sun);

    this.compass = this.buildCompass(document.documentElement.dataset.theme === "dark");
    this.scene.add(this.compass);

    this.gizmoCamera = new THREE.PerspectiveCamera(45, 16 / 9, 1, 2000);
    this.gizmoCamera.matrixAutoUpdate = false;
    this.scene.add(this.gizmoCamera);
    this.helper = new THREE.CameraHelper(this.gizmoCamera);
    this.scene.add(this.helper);
    this.eyeMarker = new THREE.Mesh(
      new THREE.SphereGeometry(6, 20, 14),
      new THREE.MeshBasicMaterial({ color: 0xbd8443 })
    );
    this.scene.add(this.eyeMarker);

    this.raycaster = new THREE.Raycaster();
    this.onPickHandler = null;
    canvas.addEventListener("dblclick", (event) => this.pick(event));

    const resize = () => {
      const rect = canvas.parentElement.getBoundingClientRect();
      const width = Math.max(1, Math.floor(rect.width));
      const height = Math.max(1, Math.floor(rect.height));
      this.renderer.setSize(width, height, false);
      this.camera.aspect = width / height;
      this.camera.updateProjectionMatrix();
    };
    new ResizeObserver(resize).observe(canvas.parentElement);
    resize();

    const loop = () => {
      this.controls.update();
      this.renderer.render(this.scene, this.camera);
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  }

  buildCompass(dark) {
    const group = new THREE.Group();
    /** @type {[string,number,number][]} */
    const labels = [["N", 0, -1900], ["S", 0, 1900], ["E", 1900, 0], ["W", -1900, 0]];
    for (const [text, x, z] of labels) {
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 64;
      const context = canvas.getContext("2d");
      context.font = "600 44px system-ui";
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.fillStyle = text === "N"
        ? (dark ? "#c96f67" : "#96423b")
        : (dark ? "#b0aca2" : "#6f6a5f");
      context.fillText(text, 32, 34);
      const sprite = new THREE.Sprite(new THREE.SpriteMaterial({
        map: new THREE.CanvasTexture(canvas),
        transparent: true,
        depthTest: false,
      }));
      sprite.scale.set(130, 130, 1);
      sprite.position.set(x, 30, z);
      group.add(sprite);
    }
    return group;
  }

  setTheme(theme) {
    const dark = theme === "dark";
    this.renderer.setClearColor(dark ? 0x131315 : 0xece9e0, 1);
    this.scene.remove(this.compass);
    for (const sprite of this.compass.children) {
      if (!(sprite instanceof THREE.Sprite)) throw new Error("Invalid compass sprite");
      sprite.material.map?.dispose();
      sprite.material.dispose();
    }
    this.compass = this.buildCompass(dark);
    this.scene.add(this.compass);
    if (typeof this.helper.setColors === "function") {
      const accent = new THREE.Color(dark ? 0xe8e3d8 : 0x201e1a);
      const warn = new THREE.Color(dark ? 0xbd8443 : 0x8a6430);
      const grid = new THREE.Color(dark ? 0x3e3c38 : 0xc7c0af);
      this.helper.setColors(accent, warn, grid, accent, grid);
    }
    this.eyeMarker.material.color.setHex(dark ? 0xbd8443 : 0x8a6430);
  }

  /* pose in geodetic terms; origin anchors the local frame. */
  setFrustum(pose, origin, aspect, farHint) {
    const world = geo.cameraWorldTransform(pose, origin);
    this.gizmoCamera.matrix.fromArray(world);
    this.gizmoCamera.matrix.decompose(
      this.gizmoCamera.position,
      this.gizmoCamera.quaternion,
      this.gizmoCamera.scale
    );
    this.gizmoCamera.fov = pose.verticalFovDegrees;
    this.gizmoCamera.aspect = aspect;
    this.gizmoCamera.near = 1;
    this.gizmoCamera.far = Math.min(Math.max(farHint || 500, 100), 30000);
    this.gizmoCamera.updateProjectionMatrix();
    this.helper.update();
    this.eyeMarker.position.set(world[12], world[13], world[14]);
  }

  focus(localPoint, distanceHint) {
    this.controls.target.set(localPoint[0], localPoint[1], localPoint[2]);
    this.compass.position.set(localPoint[0], localPoint[1], localPoint[2]);
    const span = Math.max(200, Math.min(distanceHint || 800, 20000));
    const direction = this.camera.position.clone().sub(this.controls.target);
    if (direction.lengthSq() < 1) direction.set(0.6, 0.7, 0.6);
    direction.setLength(span * 1.4);
    this.camera.position.copy(this.controls.target).add(direction);
    this.controls.update();
  }

  onPick(handler) {
    this.onPickHandler = handler;
  }

  pick(event) {
    if (!this.onPickHandler) return;
    const rect = this.canvas.getBoundingClientRect();
    const pointer = new THREE.Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    );
    this.raycaster.setFromCamera(pointer, this.camera);
    const hits = this.raycaster.intersectObjects(this.tiles.children, true);
    if (hits.length) {
      this.onPickHandler([hits[0].point.x, hits[0].point.y, hits[0].point.z]);
    }
  }

  disposeTileGroup(group) {
    group.traverse((child) => {
      if (child.isMesh) {
        child.geometry.dispose();
        if (child.material.map) child.material.map.dispose();
        child.material.dispose();
      }
    });
  }
}

/* ------------------------------------------------------------------ tiles -- */
const nextMacrotask = () => new Promise((resolve) => setTimeout(resolve, 0));

class TileLoader {
  constructor(sceneView, onProgress) {
    this.sceneView = sceneView;
    this.onProgress = onProgress;
    this.cache = new Map(); // tileUri → { group, rtcCenter, bytes }
    this.generation = 0;
    this.maxBytes = 96 * 1024 * 1024;
  }

  contentMatrix(localFromEcef, tile, rtcCenter) {
    let matrix = m4.mul(localFromEcef, m4.fromArray(tile.ecefFromContent));
    if (rtcCenter) matrix = m4.mul(matrix, m4.translation(rtcCenter));
    return matrix;
  }

  async load(manifest) {
    const generation = ++this.generation;
    const localFromEcef = m4.fromArray(manifest.localFromEcef);
    const usable = manifest.tiles.filter((tile) => !tile.oversize);
    const skipped = manifest.tiles.length - usable.length;
    const wanted = new Set(usable.map((tile) => tile.tileUri));
    for (const [uri, entry] of [...this.cache]) {
      if (!wanted.has(uri)) {
        this.sceneView.tiles.remove(entry.group);
        this.sceneView.disposeTileGroup(entry.group);
        this.cache.delete(uri);
      }
    }
    const ordered = usable
      .map((tile) => {
        const local = m4.mul(localFromEcef, m4.fromArray(tile.ecefFromContent));
        return { tile, distance: Math.hypot(local[12], local[13], local[14]) };
      })
      .sort((a, b) => a.distance - b.distance)
      .map((entry) => entry.tile);

    const mod = await draco.init();
    let done = 0;
    const failures = [];
    const queue = ordered.slice();
    const report = () => this.onProgress({
      done, total: ordered.length, skipped, failures,
      bytes: [...this.cache.values()].reduce((sum, entry) => sum + entry.bytes, 0),
    });
    report();
    const worker = async () => {
      while (queue.length) {
        if (this.generation !== generation) return;
        const tile = queue.shift();
        try {
          let entry = this.cache.get(tile.tileUri);
          if (!entry) {
            const bytes = await readBlobResource(tile.tileUri);
            if (this.generation !== generation) return;
            await nextMacrotask(); // one decode per macrotask keeps orbiting responsive
            const built = await glb.build(mod, glb.parse(bytes));
            if (this.generation !== generation) {
              this.sceneView.disposeTileGroup(built.group);
              return;
            }
            entry = { group: built.group, rtcCenter: built.rtcCenter, bytes: bytes.byteLength };
            this.cache.set(tile.tileUri, entry);
            this.sceneView.tiles.add(entry.group);
          }
          entry.group.matrix.fromArray(this.contentMatrix(localFromEcef, tile, entry.rtcCenter));
        } catch (cause) {
          failures.push(`${tile.tileUri.slice(-12)}: ${cause.message}`);
        }
        done += 1;
        report();
      }
    };
    await Promise.all([worker(), worker(), worker()]);
  }
}

/* -------------------------------------------------------------------- ui -- */


function showError(id, message) {
  const node = el(id);
  node.textContent = message;
  node.style.display = "block";
  reportSize();
}
function clearError(id) {
  el(id).style.display = "none";
}
function setStatus(text, busy) {
  el("status-text").textContent = text;
  el("status").classList.toggle("busy", Boolean(busy));
}

function bindSliderPair(sliderId, numberId, toNumber, toSlider) {
  const slider = input(sliderId);
  const number = input(numberId);
  slider.addEventListener("input", () => {
    number.value = toNumber ? toNumber(Number(slider.value)) : slider.value;
    number.dispatchEvent(new Event("change", { bubbles: true }));
  });
  number.addEventListener("input", () => {
    slider.value = toSlider ? toSlider(Number(number.value)) : number.value;
  });
}

class CameraForm {
  constructor(onChange) {
    this.mode = "orbit_target";
    this.onChange = onChange;
    for (const button of el("mode-tabs").querySelectorAll("button")) {
      button.addEventListener("click", () => this.setMode(button.dataset.mode));
    }
    bindSliderPair("orbit-distance-slider", "orbit-distance",
      (value) => Math.round(10 ** value), (value) => Math.log10(Math.max(value, 1)));
    bindSliderPair("orbit-azimuth-slider", "orbit-azimuth");
    bindSliderPair("orbit-elevation-slider", "orbit-elevation");
    bindSliderPair("pose-heading-slider", "pose-heading");
    bindSliderPair("pose-pitch-slider", "pose-pitch");
    bindSliderPair("fov-slider", "fov");
    for (const input of document.querySelectorAll("#panel input")) {
      input.addEventListener("change", () => this.changed());
      input.addEventListener("input", () => this.changed());
    }
    this.installLatLonPasteSplit("target-lat", "target-lon");
    this.installLatLonPasteSplit("eye-lat", "eye-lon");
  }

  installLatLonPasteSplit(latId, lonId) {
    el(latId).addEventListener("change", () => {
      const raw = input(latId).value;
      const match = /^\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*$/.exec(raw);
      if (match) {
        input(latId).value = match[1];
        input(lonId).value = match[2];
        this.changed();
      }
    });
  }

  setMode(mode) {
    this.mode = mode;
    for (const button of el("mode-tabs").querySelectorAll("button")) {
      button.setAttribute("aria-selected", String(button.dataset.mode === mode));
    }
    el("fs-target").hidden = mode === "pose";
    el("fs-orbit").hidden = mode !== "orbit_target";
    el("fs-eye").hidden = mode === "orbit_target";
    el("fs-eye").querySelector("legend").textContent =
      mode === "pose" ? "Position (WGS84)" : "Eye (WGS84)";
    el("fs-orientation").hidden = mode !== "pose";
    this.changed();
    reportSize();
  }

  position(prefix) {
    return {
      latitudeDegrees: Number(input(`${prefix}-lat`).value),
      longitudeDegrees: Number(input(`${prefix}-lon`).value),
      ellipsoidalHeightMeters: Number(input(`${prefix}-alt`).value),
    };
  }

  /* Exact serde shape of the server's CameraDefinition. */
  value() {
    const fov = Number(el("fov").value);
    if (this.mode === "orbit_target") {
      return {
        kind: "orbit_target",
        target: this.position("target"),
        distanceMeters: Number(el("orbit-distance").value),
        azimuthDegrees: Number(el("orbit-azimuth").value),
        elevationDegrees: Number(el("orbit-elevation").value),
        verticalFovDegrees: fov,
      };
    }
    if (this.mode === "look_at") {
      return {
        kind: "look_at",
        eye: this.position("eye"),
        target: this.position("target"),
        verticalFovDegrees: fov,
      };
    }
    return {
      kind: "pose",
      position: this.position("eye"),
      orientation: {
        headingDegrees: Number(el("pose-heading").value),
        pitchDegrees: Number(el("pose-pitch").value),
        rollDegrees: Number(el("pose-roll").value),
      },
      verticalFovDegrees: fov,
    };
  }

  setTarget(position) {
    el("target-lat").value = position.latitudeDegrees.toFixed(6);
    el("target-lon").value = position.longitudeDegrees.toFixed(6);
    el("target-alt").value = position.ellipsoidalHeightMeters.toFixed(1);
    this.changed();
  }

  setDefinition(definition) {
    el("fov").value = String(definition.verticalFovDegrees);
    el("fov-slider").value = String(definition.verticalFovDegrees);
    if (definition.kind === "orbit_target") {
      el("target-lat").value = String(definition.target.latitudeDegrees);
      el("target-lon").value = String(definition.target.longitudeDegrees);
      el("target-alt").value = String(definition.target.ellipsoidalHeightMeters);
      el("orbit-distance").value = String(definition.distanceMeters);
      el("orbit-distance-slider").value = String(Math.log10(Math.max(definition.distanceMeters, 1)));
      el("orbit-azimuth").value = String(definition.azimuthDegrees);
      el("orbit-azimuth-slider").value = String(definition.azimuthDegrees);
      el("orbit-elevation").value = String(definition.elevationDegrees);
      el("orbit-elevation-slider").value = String(definition.elevationDegrees);
    } else if (definition.kind === "look_at") {
      el("eye-lat").value = String(definition.eye.latitudeDegrees);
      el("eye-lon").value = String(definition.eye.longitudeDegrees);
      el("eye-alt").value = String(definition.eye.ellipsoidalHeightMeters);
      el("target-lat").value = String(definition.target.latitudeDegrees);
      el("target-lon").value = String(definition.target.longitudeDegrees);
      el("target-alt").value = String(definition.target.ellipsoidalHeightMeters);
    } else {
      el("eye-lat").value = String(definition.position.latitudeDegrees);
      el("eye-lon").value = String(definition.position.longitudeDegrees);
      el("eye-alt").value = String(definition.position.ellipsoidalHeightMeters);
      el("pose-heading").value = String(definition.orientation.headingDegrees);
      el("pose-heading-slider").value = String(definition.orientation.headingDegrees);
      el("pose-pitch").value = String(definition.orientation.pitchDegrees);
      el("pose-pitch-slider").value = String(definition.orientation.pitchDegrees);
      el("pose-roll").value = String(definition.orientation.rollDegrees);
    }
    this.setMode(definition.kind);
  }

  changed() {
    if (this.onChange) this.onChange();
  }
}

function capturePolicy() {
  return {
    widthPx: Number(el("cap-width").value),
    heightPx: Number(el("cap-height").value),
    maxScreenErrorPx: Number(el("cap-sse").value),
    deadlineMs: Math.round(Number(el("cap-deadline").value) * 1000),
    deadlineBehavior: el("cap-behavior").value,
    encoding: el("cap-encoding").value,
  };
}

/* -------------------------------------------------------------------- app -- */
/** @type {[string,unknown][]} */
const PRESETS = [
  ["Statue of Liberty · orbit 650 m", { kind: "orbit_target",
    target: { latitudeDegrees: 40.6892, longitudeDegrees: -74.0445, ellipsoidalHeightMeters: 30 },
    distanceMeters: 650, azimuthDegrees: 210, elevationDegrees: 40, verticalFovDegrees: 45 }],
  ["Manhattan skyline · look-at", { kind: "look_at",
    eye: { latitudeDegrees: 40.6935, longitudeDegrees: -74.0270, ellipsoidalHeightMeters: 350 },
    target: { latitudeDegrees: 40.7075, longitudeDegrees: -74.0113, ellipsoidalHeightMeters: 150 },
    verticalFovDegrees: 55 }],
  ["Golden Gate · orbit 1.2 km", { kind: "orbit_target",
    target: { latitudeDegrees: 37.8199, longitudeDegrees: -122.4786, ellipsoidalHeightMeters: 80 },
    distanceMeters: 1200, azimuthDegrees: 135, elevationDegrees: 25, verticalFovDegrees: 50 }],
];

const APP_TOOL_NAMES = new Set([
  "create_scene_composition", "create_view", "set_camera", "capture_frame", "close_view"
]);

const app = {
  layers: /** @type {import("./generated/view").AppContracts["layers"]} */ ([]),
  composition: /** @type {import("./generated/view").AppContracts["composition"]|null} */ (null),
  view: /** @type {import("./generated/view").AppContracts["view"]|null} */ (null),          // ViewRecord from the server
  manifest: /** @type {import("./generated/view").AppContracts["scene"]|null} */ (null),      // PreviewSceneRecord
  localFromEcef: null, // Float64Array(16)
  captureTaskId: null,
  cancelRequested: false,
  toolPrefix: "",
  ready: false,
  pendingToolInput: null,
  pendingToolResult: null,
};

let sceneView = null;
let tileLoader = null;
let form = null;

function configureToolProjection(hostContext) {
  const publishedName = hostContext && hostContext.toolInfo &&
    hostContext.toolInfo.tool && hostContext.toolInfo.tool.name;
  if (typeof publishedName !== "string") return;
  for (const localName of APP_TOOL_NAMES) {
    if (publishedName === localName) {
      app.toolPrefix = "";
      return;
    }
    const suffix = `__${localName}`;
    if (publishedName.endsWith(suffix)) {
      app.toolPrefix = publishedName.slice(0, -localName.length);
      return;
    }
  }
}

function projectedToolName(localName) {
  if (!APP_TOOL_NAMES.has(localName)) {
    console.error(`tool ${localName} is not available to this app`);
    throw new Error("This app isn't allowed to use that tool.");
  }
  return `${app.toolPrefix}${localName}`;
}

function structuredArgument(value) {
  if (typeof value !== "string") return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function applyInitialToolInput(params) {
  const args = params && params.arguments;
  if (!args || !form) return;
  const camera = structuredArgument(args.camera);
  if (camera && typeof camera === "object") form.setDefinition(camera);
  if (typeof args.baseLayer === "string") {
    const layer = el("layer-select");
    if ([...layer.options].some((option) => option.value === args.baseLayer)) {
      layer.value = args.baseLayer;
    }
  }
}

async function applyInitialToolResult(result) {
  if (result && result.isError) {
    showError("action-error", toolFailureText(result, "initial tool"));
    return;
  }
  const record = structuredResult(result);
  if (!record || typeof record !== "object") return;
  if (typeof record.compositionId === "string" &&
      typeof record.compositionUri === "string" &&
      typeof record.baseLayer === "string") {
    app.composition = admit("composition", record);
    const layer = el("layer-select");
    if ([...layer.options].some((option) => option.value === record.baseLayer)) {
      layer.value = record.baseLayer;
    }
    updateViewChip();
    setStatus(`composition ${record.compositionId.slice(0, 8)}… ready — create a view`, false);
    return;
  }
  if (typeof record.viewId === "string" && typeof record.viewUri === "string" &&
      record.camera && record.resolvedCamera) {
    applyViewRecord(record);
    setStatus(`created view ${record.viewId}`, false);
    await refreshScene();
  }
}

function flushInitialToolData() {
  if (!app.ready) return;
  if (app.pendingToolInput) {
    applyInitialToolInput(app.pendingToolInput);
    app.pendingToolInput = null;
  }
  if (app.pendingToolResult) {
    const result = app.pendingToolResult;
    app.pendingToolResult = null;
    void applyInitialToolResult(result);
  }
}

function sceneOrigin() {
  if (app.manifest) return app.manifest.localOrigin;
  try {
    return geo.resolveCamera(form.value()).position;
  } catch {
    return { latitudeDegrees: 0, longitudeDegrees: 0, ellipsoidalHeightMeters: 0 };
  }
}

function cameraTargetPosition(definition) {
  if (definition.kind === "orbit_target" || definition.kind === "look_at") return definition.target;
  return null;
}

function updateResolvedStrip(pose, confirmed) {
  const node = el("resolved");
  node.classList.toggle("confirmed", Boolean(confirmed));
  node.textContent =
    `${confirmed ? "server" : "local"} · lat ${pose.position.latitudeDegrees.toFixed(6)}° ` +
    `lon ${pose.position.longitudeDegrees.toFixed(6)}° alt ${pose.position.ellipsoidalHeightMeters.toFixed(1)} m · ` +
    `H ${pose.orientation.headingDegrees.toFixed(1)}° P ${pose.orientation.pitchDegrees.toFixed(1)}° ` +
    `R ${pose.orientation.rollDegrees.toFixed(1)}° · FOV ${pose.verticalFovDegrees}°`;
}

function updateFrustum() {
  clearError("camera-error");
  let pose;
  const definition = form.value();
  try {
    pose = geo.resolveCamera(definition);
  } catch (cause) {
    showError("camera-error", cause.message);
    return;
  }
  updateResolvedStrip(pose, false);
  const origin = sceneOrigin();
  const aspect = Number(el("cap-width").value) / Math.max(Number(el("cap-height").value), 1);
  let farHint = 800;
  const target = cameraTargetPosition(definition);
  if (target) {
    const eyeEcef = geo.geodeticToEcef(pose.position);
    const targetEcef = geo.geodeticToEcef(target);
    farHint = Math.hypot(
      eyeEcef[0] - targetEcef[0], eyeEcef[1] - targetEcef[1], eyeEcef[2] - targetEcef[2]
    ) * 1.6;
  } else {
    farHint = Math.max(pose.position.ellipsoidalHeightMeters * 3, 500);
  }
  sceneView.setFrustum(pose, origin, aspect, farHint);
}

function updateViewChip() {
  const chip = el("view-chip");
  if (!app.view) {
    if (app.composition) {
      chip.textContent = "composition ready";
      chip.className = "chip ok";
      chip.title = app.composition.compositionUri;
    } else {
      chip.textContent = "no view";
      chip.className = "chip";
      chip.removeAttribute("title");
    }
  } else {
    chip.textContent = `${app.view.viewId.slice(0, 8)}… r${app.view.revision}`;
    chip.className = "chip ok";
    chip.title = app.view.viewUri;
  }
  el("btn-create").hidden = Boolean(app.view);
  el("btn-apply").hidden = !app.view;
  el("btn-close").hidden = !app.view;
  el("btn-capture").disabled = !app.view || Boolean(app.captureTaskId);
  el("btn-refresh-scene").disabled = !app.view;
}

/** @param {import("./generated/view").AppContracts["view"]} record */
function applyViewRecord(record) {
  record=admit("view",record);
  app.view = record;
  form.setDefinition(record.camera);
  updateResolvedStrip(record.resolvedCamera, true);
  updateViewChip();
}

async function loadLayers() {
  app.layers = await readJsonResource("view://layers","layers");
  const select = el("layer-select");
  select.replaceChildren();
  for (const layer of app.layers) {
    const option = document.createElement("option");
    option.value = layer.layerId;
    option.textContent = `${layer.label} (${layer.sourceKind})`;
    select.append(option);
  }
}

async function createView() {
  const camera = form.value();
  setStatus("creating view…", true);
  try {
    const selectedLayer = el("layer-select").value;
    const composition = app.composition && app.composition.baseLayer === selectedLayer
      ? app.composition
      : await callTool("create_scene_composition", {
          schemaVersion: 2,
          baseLayer: selectedLayer,
          mapReleases: [],
          styleId: "view-preview:1",
          governedInputs: [],
          overlays: [],
        });
    app.composition = composition;
    const record = await callTool("create_view", {
      compositionId: composition.compositionId,
      camera,
    });
    applyViewRecord(record);
    setStatus(`created view ${record.viewId}`, false);
    await refreshScene();
  } catch (cause) {
    setStatus("idle", false);
    showError("action-error", `Couldn't create the view: ${cause.message}`);
  }
}

async function applyCamera() {
  if (!app.view) return;
  setStatus("applying camera…", true);
  try {
    const record = await callTool("set_camera", {
      viewId: app.view.viewId,
      expectedRevision: app.view.revision,
      camera: form.value(),
    });
    applyViewRecord(record);
    setStatus(`camera applied at revision ${record.revision}`, false);
    await refreshScene();
  } catch (cause) {
    setStatus("idle", false);
    showError("action-error", `Couldn't apply the camera: ${cause.message}`);
    void resyncView();
  }
}

async function resyncView() {
  if (!app.view) return;
  try {
    const record = await readJsonResource(app.view.viewUri,"view");
    applyViewRecord(record);
  } catch {
    app.view = null;
    updateViewChip();
  }
}

async function refreshScene() {
  if (!app.view) return;
  clearError("action-error");
  setStatus("loading scene manifest…", true);
  el("tile-chip").textContent = "loading…";
  try {
    const policy = capturePolicy();
    const query = new URLSearchParams({
      width_px: String(policy.widthPx),
      height_px: String(policy.heightPx),
      max_screen_error_px: String(policy.maxScreenErrorPx),
    });
    const manifest = await readJsonResource(
      `view://view/${app.view.viewId}/scene?${query}`,"scene"
    );
    if(manifest.compositionId!==app.view.compositionId || manifest.viewRevision!==app.view.revision) throw new Error("Scene manifest differs from active view");
    app.manifest = manifest;
    app.localFromEcef = m4.fromArray(manifest.localFromEcef);
    const attribution = el("scene-attribution");
    attribution.hidden = !manifest.attribution.lines.length;
    attribution.textContent = manifest.attribution.lines.join(" · ");
    const definition = form.value();
    const target = cameraTargetPosition(definition) || manifest.resolvedCamera.position;
    const targetLocal = m4.transformPoint(app.localFromEcef, geo.geodeticToEcef(target));
    let focusDistance = 800;
    if (definition.kind === "orbit_target") focusDistance = definition.distanceMeters;
    sceneView.focus(targetLocal, focusDistance);
    updateFrustum();
    await tileLoader.load(manifest);
    setStatus(
      manifest.detailComplete ? "scene loaded" : "scene loaded (partial detail)",
      false
    );
  } catch (cause) {
    setStatus("idle", false);
    el("tile-chip").textContent = "scene unavailable";
    showError("action-error", `Couldn't load the scene: ${cause.message}`);
  }
}

function tileProgress(progress) {
  const mb = (progress.bytes / (1024 * 1024)).toFixed(0);
  el("tile-chip").textContent =
    `${progress.done}/${progress.total} tiles · ${mb} MB` +
    (progress.skipped ? ` · ${progress.skipped} oversize` : "");
  const diagnostics = el("diagnostics");
  if (progress.failures.length) {
    diagnostics.hidden = false;
    diagnostics.textContent = `tile failures: ${progress.failures.join("; ")}`;
  } else {
    diagnostics.hidden = true;
  }
}

const TERMINAL_STATUSES = new Set(["completed", "failed", "cancelled"]);

async function captureFrame() {
  if (!app.view || app.captureTaskId) return;
  clearError("action-error");
  app.cancelRequested = false;
  el("btn-cancel").hidden = false;
  el("btn-capture").disabled = true;
  try {
    const created = await callToolRaw(
      "capture_frame",
      {
        viewId: app.view.viewId,
        expectedRevision: app.view.revision,
        sceneTime: new Date().toISOString(),
        policy: capturePolicy(),
      }
    );
    const task = created;
    const taskId = task?.taskId;
    if (!taskId) throw new Error("The host didn't start the capture. Try again.");
    app.captureTaskId = taskId;
    const pollInterval = Math.min(Math.max(task.pollIntervalMs || 500, 250), 5000);
    setStatus(`capture task ${taskId.slice(0, 8)}… queued`, true);
    let snapshot = task;
    let status = task.status || "working";
    let statusMessage = task.statusMessage || "";
    while (!TERMINAL_STATUSES.has(status)) {
      await new Promise((resolve) => setTimeout(resolve, pollInterval));
      const current = await tasks.get(app.captureTaskId);
      snapshot = current;
      status = snapshot.status || status;
      statusMessage = snapshot.statusMessage || "";
      setStatus(`capture ${status}${statusMessage ? ` — ${statusMessage}` : ""}`, true);
    }
    if (status === "failed") throw new Error(statusMessage || "The capture failed.");
    if (status === "cancelled") {
      setStatus("capture cancelled", false);
      return;
    }
    const result = snapshot.result || {};
    if (result.isError) throw new Error(toolFailureText(result, "capture_frame"));
    applyFrameResult(result);
    setStatus("frame captured", false);
  } catch (cause) {
    setStatus("idle", false);
    showError("action-error", `Couldn't capture a frame: ${cause.message}`);
    void resyncView();
  } finally {
    app.captureTaskId = null;
    el("btn-cancel").hidden = true;
    updateViewChip();
  }
}

async function cancelCapture() {
  if (!app.captureTaskId || app.cancelRequested) return;
  app.cancelRequested = true;
  try {
    await tasks.cancel(app.captureTaskId);
    setStatus("cancel requested…", true);
  } catch (cause) {
    showError("action-error", `Couldn't cancel the capture: ${cause.message}`);
  }
}

function applyFrameResult(result) {
  const content = Array.isArray(result.content) ? result.content : [];
  const image = content.find((entry) => entry && entry.type === "image");
  const record = toolValue("capture_frame",structuredResult(result),{viewId:app.view?.viewId,expectedRevision:app.view?.revision,compositionId:app.view?.compositionId});
  if (image) {
    el("frame-img").src = `data:${image.mimeType || "image/jpeg"};base64,${image.data}`;
    el("frame-img").hidden = false;
    el("frame-empty").hidden = true;
  }
  if (record) {
    const meta = el("frame-meta");
    meta.hidden = false;
    meta.replaceChildren();
    const rows = [
      ["frame", record.frameUri],
      ["captured", record.capturedAt],
      ["size", `${record.widthPx}×${record.heightPx} · ${(record.byteLength / 1024).toFixed(0)} KiB ${record.mimeType}`],
      ["revision", `r${record.viewRevision}`],
      ["tiles", `${record.visibleTileCount} visible · ${record.pendingTileCount} pending`],
      ["detail", record.detailComplete ? "complete" : `partial (sse ${record.actualMaxScreenErrorPx.toFixed(1)}px)`],
      ["pose", `lat ${record.resolvedCamera.position.latitudeDegrees.toFixed(6)} lon ${record.resolvedCamera.position.longitudeDegrees.toFixed(6)} alt ${record.resolvedCamera.position.ellipsoidalHeightMeters.toFixed(1)}`],
      ["hpr", `${record.resolvedCamera.orientation.headingDegrees.toFixed(1)} / ${record.resolvedCamera.orientation.pitchDegrees.toFixed(1)} / ${record.resolvedCamera.orientation.rollDegrees.toFixed(1)}`],
    ];
    for (const [key, value] of rows) {
      const dt = document.createElement("dt");
      dt.textContent = key;
      const dd = document.createElement("dd");
      dd.textContent = String(value);
      meta.append(dt, dd);
    }
    const attribution = el("frame-attribution");
    attribution.hidden = !record.attribution.lines.length;
    attribution.textContent = record.attribution.lines.join(" · ");
  }
  selectPane("frame-pane");
}

async function closeView(silent) {
  if (!app.view) return;
  const request = { viewId: app.view.viewId, expectedRevision: app.view.revision };
  app.view = null;
  app.manifest = null;
  updateViewChip();
  el("tile-chip").textContent = "no scene";
  try {
    await callTool("close_view", request);
    if (!silent) setStatus("view closed", false);
  } catch (cause) {
    if (!silent) showError("action-error", `Couldn't close the view: ${cause.message}`);
  }
}

function selectPane(paneId) {
  for (const button of el("stage-tabs").querySelectorAll("button")) {
    button.setAttribute("aria-selected", String(button.dataset.pane === paneId));
  }
  el("scene-pane").hidden = paneId !== "scene-pane";
  el("frame-pane").hidden = paneId !== "frame-pane";
  reportSize();
}

function copyCameraJson() {
  const text = JSON.stringify(form.value(), null, 2);
  const finish = () => setStatus("camera JSON copied", false);
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(finish, () => fallbackCopy(text, finish));
  } else {
    fallbackCopy(text, finish);
  }
}
function fallbackCopy(text, finish) {
  const area = document.createElement("textarea");
  area.value = text;
  document.body.append(area);
  area.select();
  document.execCommand("copy");
  area.remove();
  finish();
}

/* ------------------------------------------------------------- lifecycle -- */
function reportSize() {
  bridge.notify("ui/notifications/size-changed", {
    height: Math.ceil(document.documentElement.getBoundingClientRect().height) + 8,
  });
}

function applyHostContext(context) {
  if (!context) return;
  configureToolProjection(context);
  if (context.theme === "dark" || context.theme === "light") {
    document.documentElement.dataset.theme = context.theme;
    if (sceneView) sceneView.setTheme(context.theme);
  }
}

bridge.on("ui/notifications/host-context-changed", (params) => {
  applyHostContext(params && (params.hostContext || params));
});
bridge.on("ui/notifications/tool-result", (params) => {
  app.pendingToolResult = toolEnvelope(params?.result || params);
  flushInitialToolData();
});
bridge.on("ui/notifications/tool-input", (params) => {
  app.pendingToolInput = params;
  flushInitialToolData();
});
bridge.on("ui/notifications/tool-cancelled", () => {});
bridge.on("ui/resource-teardown", (_params, id) => {
  void closeView(true);
  if (id !== undefined) bridge.post({ jsonrpc: "2.0", id, result: {} });
});

(async () => {
  try {
    const initialized = await bridge.request("ui/initialize", {
      protocolVersion: "2026-01-26",
      appInfo: { name: "view-preview", version: "1.0.0" },
      appCapabilities: { availableDisplayModes: ["inline"] },
    });
    bridge.notify("ui/notifications/initialized", {});
    el("boot").hidden = true;
    el("app").hidden = false;

    sceneView = new SceneView(el("gl"));
    tileLoader = new TileLoader(sceneView, tileProgress);
    form = new CameraForm(updateFrustum);
    applyHostContext(initialized && initialized.hostContext);

    el("btn-create").addEventListener("click", () => { void createView(); });
    el("btn-apply").addEventListener("click", () => { void applyCamera(); });
    el("btn-capture").addEventListener("click", () => { void captureFrame(); });
    el("btn-cancel").addEventListener("click", () => { void cancelCapture(); });
    el("btn-refresh-scene").addEventListener("click", () => { void refreshScene(); });
    el("btn-close").addEventListener("click", () => { void closeView(false); });
    el("btn-copy").addEventListener("click", copyCameraJson);
    for (const button of el("stage-tabs").querySelectorAll("button")) {
      button.addEventListener("click", () => selectPane(button.dataset.pane));
    }
    for (const [label, definition] of PRESETS) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.addEventListener("click", () => {
        form.setDefinition(JSON.parse(JSON.stringify(definition)));
      });
      el("preset-list").append(button);
    }
    sceneView.onPick((localPoint) => {
      if (!app.localFromEcef) return;
      const ecef = m4.transformPoint(m4.invertRigid(app.localFromEcef), localPoint);
      const position = geo.ecefToGeodetic(ecef);
      if (form.mode === "pose") {
        try {
          const eye = form.position("eye");
          const orientation = geo.orientationToward(eye, position, Number(el("pose-roll").value));
          el("pose-heading").value = String(orientation.headingDegrees.toFixed(1));
          el("pose-heading-slider").value = String(orientation.headingDegrees);
          el("pose-pitch").value = String(orientation.pitchDegrees.toFixed(1));
          el("pose-pitch-slider").value = String(orientation.pitchDegrees);
          form.changed();
        } catch {}
      } else {
        form.setTarget(position);
      }
    });

    await draco.init();
    await loadLayers();
    app.ready = true;
    updateViewChip();
    updateFrustum();
    setStatus("ready — create a view to load the scene", false);
    flushInitialToolData();
    reportSize();
  } catch (cause) {
    el("boot").hidden = true;
    showError("boot-error", `The View app couldn't connect to its host. Reload to try again. (${cause.message})`);
  }
})();
