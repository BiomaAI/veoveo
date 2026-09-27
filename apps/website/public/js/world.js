// Live world behind the page. A point-cloud terrain carries radiating pulses, sensor
// footprints and soft lights under each robot. Relay towers carry RF rings and data
// packets between the robots and a holographic replica of the world model. Every few
// seconds a scripted event runs the loop: an anomaly appears, a drone detects it, the
// world model updates, and the nearest quadruped is dispatched. Page scroll moves the
// camera between the page sections.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';

const canvas = document.getElementById('world');
const small = window.matchMedia('(max-width: 760px)').matches;

function hasWebGL() {
    try {
        return !!document.createElement('canvas').getContext('webgl2');
    } catch {
        return false;
    }
}


/* ---------- terrain field ---------- */

function hash(x, z) {
    const s = Math.sin(x * 127.1 + z * 311.7) * 43758.5453;
    return s - Math.floor(s);
}

function noise(x, z) {
    const xi = Math.floor(x), zi = Math.floor(z);
    const xf = x - xi, zf = z - zi;
    const u = xf * xf * (3 - 2 * xf), v = zf * zf * (3 - 2 * zf);
    const a = hash(xi, zi), b = hash(xi + 1, zi), c = hash(xi, zi + 1), d = hash(xi + 1, zi + 1);
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
}

function canalZ(x) {
    return 46 + Math.sin(x / 58) * 18;
}

function height(x, z) {
    let amp = 1, freq = 0.011, sum = 0;
    for (let i = 0; i < 5; i++) {
        sum += amp * noise(x * freq, z * freq);
        amp *= 0.5;
        freq *= 2.05;
    }
    const ridge = Math.abs(noise(x * 0.006 + 13.1, z * 0.006 - 7.7) - 0.5) * 2;
    let h = (sum - 0.9) * 30 + ridge * 22;
    const plateau = Math.exp(-(x * x + z * z) / 2600);
    h = h * (1 - plateau) + 2 * plateau;
    const canal = Math.exp(-Math.pow(z - canalZ(x), 2) / 60);
    return h * (1 - canal) - 4 * canal;
}

function onLand(x, z) {
    return Math.abs(z - canalZ(x)) > 10;
}

function radialTexture(inner, outer) {
    const c = document.createElement('canvas');
    c.width = c.height = 128;
    const g = c.getContext('2d');
    const grad = g.createRadialGradient(64, 64, 0, 64, 64, 64);
    grad.addColorStop(0, inner);
    grad.addColorStop(1, outer);
    g.fillStyle = grad;
    g.fillRect(0, 0, 128, 128);
    return new THREE.CanvasTexture(c);
}

function labelTexture(text) {
    const c = document.createElement('canvas');
    c.width = 256;
    c.height = 48;
    const g = c.getContext('2d');
    g.fillStyle = 'rgba(20, 8, 32, 0.85)';
    g.fillRect(0, 0, 256, 48);
    g.strokeStyle = '#c9a3e6';
    g.lineWidth = 2;
    g.strokeRect(1, 1, 254, 46);
    g.fillStyle = '#f3e8ff';
    g.font = '600 24px ui-monospace, Menlo, monospace';
    g.textBaseline = 'middle';
    g.fillText(text, 12, 25);
    const t = new THREE.CanvasTexture(c);
    t.colorSpace = THREE.SRGBColorSpace;
    return t;
}

const additive = { transparent: true, depthWrite: false, blending: THREE.AdditiveBlending };
let dotTexture = null;
const dot = () => (dotTexture ||= radialTexture('rgba(255,255,255,1)', 'rgba(255,255,255,0)'));

/* ---------- scene ---------- */

async function start() {
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5));
    renderer.setClearColor(0x04030a, 1);
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;

    // A soft veil darkens the scene behind the content column once the page scrolls.
    const veil = document.createElement('div');
    veil.id = 'world-veil';
    canvas.after(veil);

    const scene = new THREE.Scene();
    scene.fog = new THREE.FogExp2(0x04030a, 0.0034);
    const camera = new THREE.PerspectiveCamera(48, 1, 0.5, 1200);

    scene.add(new THREE.HemisphereLight(0xb9a3e6, 0x0a0612, 1.4));
    const key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(60, 120, 80);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0xb04be0, 2.2);
    rim.position.set(-90, 40, -70);
    scene.add(rim);

    const SIZE = 480;
    const GRID = small ? 160 : 250;
    const MAX_DRONES = 8, MAX_GLOWS = 16, MAX_FRUSTA = 8, MAX_LIDAR = 4, TOWERS = 3;

    /* Relay towers stand on the highest ground in a ring around the plateau. */
    const towerSpots = [];
    for (let k = 0; k < TOWERS; k++) {
        let best = null;
        for (let s = 0; s < 40; s++) {
            // Bearings stay clear of the band behind the replica as seen from the camera arc.
            const bearings = [150, 330, 20];
            const a = THREE.MathUtils.degToRad(bearings[k]) + (hash(k, s) - 0.5) * 0.5;
            const r = 105 + hash(s, k) * 45;
            const x = Math.cos(a) * r, z = Math.sin(a) * r;
            if (!onLand(x, z)) continue;
            const y = height(x, z);
            if (!best || y > best.y) best = { x, y, z };
        }
        towerSpots.push(best);
    }
    const TOWER_HEIGHT = 22, TOWER_RANGE = 95;

    /* Point-cloud terrain. */
    const count = GRID * GRID;
    const positions = new Float32Array(count * 3);
    const heights = new Float32Array(count);
    let n = 0;
    for (let gz = 0; gz < GRID; gz++) {
        for (let gx = 0; gx < GRID; gx++) {
            const x = (gx / (GRID - 1) - 0.5) * SIZE + (hash(gx, gz) - 0.5) * 0.8;
            const z = (gz / (GRID - 1) - 0.5) * SIZE + (hash(gz, gx) - 0.5) * 0.8;
            const y = height(x, z);
            positions.set([x, y, z], n * 3);
            heights[n++] = y;
        }
    }
    const terrainGeometry = new THREE.BufferGeometry();
    terrainGeometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    terrainGeometry.setAttribute('aHeight', new THREE.BufferAttribute(heights, 1));

    const vec4s = (k) => Array.from({ length: k }, () => new THREE.Vector4(0, 0, 0, 0));
    const u = {
        uTime: { value: 0 },
        uPixel: { value: renderer.getPixelRatio() },
        uFog: { value: scene.fog.density },
        uDrones: { value: vec4s(MAX_DRONES) },
        uGlows: { value: vec4s(MAX_GLOWS) },
        uFrusta: { value: vec4s(MAX_FRUSTA) },
        uLidar: { value: vec4s(MAX_LIDAR) },
        uTowers: { value: towerSpots.map((t, k) => new THREE.Vector4(t.x, t.z, k * 30, 1)) },
        uEvent: { value: new THREE.Vector4(0, 0, -100, 0) },
    };
    const terrainMaterial = new THREE.ShaderMaterial({
        ...additive,
        uniforms: u,
        vertexShader: `
            uniform float uTime, uPixel, uFog;
            uniform vec4 uDrones[${MAX_DRONES}];
            uniform vec4 uGlows[${MAX_GLOWS}];
            uniform vec4 uFrusta[${MAX_FRUSTA}];
            uniform vec4 uLidar[${MAX_LIDAR}];
            uniform vec4 uTowers[${TOWERS}];
            uniform vec4 uEvent;
            attribute float aHeight;
            varying vec3 vColor;
            varying float vAlpha;
            float angleDiff(float a, float b) { return abs(mod(a - b + 3.14159, 6.28318) - 3.14159); }
            void main() {
                vec4 mv = modelViewMatrix * vec4(position, 1.0);
                vec2 p = position.xz;
                float h = clamp((aHeight + 24.0) / 70.0, 0.0, 1.0);
                vec3 color = mix(vec3(0.20, 0.07, 0.34), vec3(0.62, 0.50, 0.86), h) * (0.55 + 0.45 * h);
                float size = 2.0;

                // Two slow pulses radiate from the plateau under the world model.
                float ring = 0.0;
                for (int k = 0; k < 2; k++) {
                    float r = mod(uTime * (30.0 + float(k) * 12.0) + float(k) * 140.0, 300.0);
                    ring += exp(-abs(length(p) - r) * 0.32) * smoothstep(300.0, 80.0, r);
                }
                // Each relay tower sends its own RF ring across the ground.
                for (int k = 0; k < ${TOWERS}; k++) {
                    vec4 t = uTowers[k];
                    float r = mod(uTime * 18.0 + t.z, ${TOWER_RANGE}.0);
                    ring += exp(-abs(length(p - t.xy) - r) * 0.5) * (1.0 - r / ${TOWER_RANGE}.0) * 0.7;
                }
                color += vec3(0.78, 0.40, 1.0) * ring * 0.8;
                size += ring * 1.2;

                // Drone sensor footprints.
                float hot = 0.0;
                for (int s = 0; s < ${MAX_DRONES}; s++) {
                    vec4 d = uDrones[s];
                    vec2 q = p - d.xy;
                    hot += exp(-dot(q, q) / max(d.z, 0.001)) * d.w;
                }
                hot = clamp(hot, 0.0, 1.0);
                color += vec3(0.95, 0.80, 1.0) * hot * 0.7;
                size += hot * 2.0;

                // A soft pool of light under every robot and operator.
                float glow = 0.0;
                for (int s = 0; s < ${MAX_GLOWS}; s++) {
                    vec4 g = uGlows[s];
                    vec2 q = p - g.xy;
                    glow += exp(-dot(q, q) / 18.0) * g.w;
                }
                color += vec3(0.85, 0.55, 1.0) * glow * 0.9;
                size += glow * 1.5;

                // RGB-D camera frustums tint the ground they see by depth.
                for (int s = 0; s < ${MAX_FRUSTA}; s++) {
                    vec4 f = uFrusta[s];
                    vec2 q = p - f.xy;
                    float d = length(q);
                    float inView = step(1.5, d) * step(d, 13.0) * step(angleDiff(atan(q.x, q.y), f.z), 0.4) * f.w;
                    vec3 depth = mix(vec3(1.0, 0.55, 0.85), vec3(0.35, 0.6, 1.0), d / 13.0);
                    color = mix(color, depth * 1.3, inView * 0.85);
                    size += inView * 1.2;
                }

                // Rover LiDAR sweeps a bright line around each rover.
                for (int s = 0; s < ${MAX_LIDAR}; s++) {
                    vec4 l = uLidar[s];
                    vec2 q = p - l.xy;
                    float d = length(q);
                    float beam = step(d, 22.0) * exp(-angleDiff(atan(q.x, q.y), l.z) * 30.0) * l.w;
                    color += vec3(0.75, 0.95, 1.0) * beam * 1.2;
                    size += beam * 1.8;
                }

                // The event pulse spreads from an anomaly when it appears.
                if (uEvent.w > 0.5) {
                    float age = uTime - uEvent.z;
                    float r = age * 32.0;
                    float e = exp(-abs(length(p - uEvent.xy) - r) * 0.4) * smoothstep(90.0, 10.0, r);
                    e += exp(-length(p - uEvent.xy) * 0.35) * 0.8;
                    color += vec3(1.0, 0.6, 0.9) * e;
                    size += e * 1.5;
                }

                vColor = color;
                float depth = -mv.z;
                float fog = 1.0 - exp(-uFog * uFog * depth * depth);
                vAlpha = (0.72 + hot * 0.28) * (1.0 - fog * 0.9);
                gl_PointSize = size * uPixel * (240.0 / max(depth, 1.0));
                gl_Position = projectionMatrix * mv;
            }
        `,
        fragmentShader: `
            varying vec3 vColor;
            varying float vAlpha;
            void main() {
                float a = smoothstep(0.5, 0.08, length(gl_PointCoord - 0.5)) * vAlpha;
                gl_FragColor = vec4(vColor * a, a);
            }
        `,
    });
    scene.add(new THREE.Points(terrainGeometry, terrainMaterial));

    /* Relay towers: lattice masts with RF rings at the top. */
    const towers = [];
    const latticeMaterial = new THREE.LineBasicMaterial({ color: 0xc9a3e6, transparent: true, opacity: 0.75 });
    function lattice() {
        const pts = [];
        const leg = (a, t) => {
            const w = 1.4 * (1 - t) + 0.25 * t;
            return new THREE.Vector3(Math.cos(a) * w, t * TOWER_HEIGHT, Math.sin(a) * w);
        };
        const corners = [0, 1, 2, 3].map((i) => Math.PI / 4 + (i * Math.PI) / 2);
        const steps = 8;
        for (let s = 0; s < steps; s++) {
            const t0 = s / steps, t1 = (s + 1) / steps;
            for (let c = 0; c < 4; c++) {
                const a = corners[c], b = corners[(c + 1) % 4];
                pts.push(leg(a, t0), leg(a, t1));
                pts.push(leg(a, t0), leg(b, t1));
                pts.push(leg(a, t1), leg(b, t1));
            }
        }
        return new THREE.BufferGeometry().setFromPoints(pts);
    }
    const latticeGeometry = lattice();
    const beaconMaterial = new THREE.MeshBasicMaterial({ color: 0xf0d8ff });
    const circle = new THREE.BufferGeometry().setFromPoints(
        Array.from({ length: 65 }, (_, i) => new THREE.Vector3(Math.cos((i / 64) * Math.PI * 2), 0, Math.sin((i / 64) * Math.PI * 2))),
    );
    for (const spot of towerSpots) {
        const group = new THREE.Group();
        group.position.set(spot.x, spot.y, spot.z);
        group.add(new THREE.LineSegments(latticeGeometry, latticeMaterial));
        const beacon = new THREE.Mesh(new THREE.SphereGeometry(0.45, 12, 8), beaconMaterial);
        beacon.position.y = TOWER_HEIGHT + 0.4;
        group.add(beacon);
        const rings = [0, 1, 2].map(() => {
            const ring = new THREE.Line(circle, new THREE.LineBasicMaterial({ color: 0xc084fc, ...additive, opacity: 0.5 }));
            ring.position.y = TOWER_HEIGHT;
            group.add(ring);
            return ring;
        });
        scene.add(group);
        towers.push({ group, beacon, rings, top: new THREE.Vector3(spot.x, spot.y + TOWER_HEIGHT, spot.z) });
    }

    /* The world model: a holographic replica of the terrain above the plateau. */
    const REPLICA_Y = 50, REPLICA_SCALE = 0.078, REPLICA_SPAN = 200;
    const replica = new THREE.Group();
    replica.position.set(0, REPLICA_Y, 0);
    scene.add(replica);
    const toReplica = (x, y, z, out = new THREE.Vector3()) =>
        out.set(x * REPLICA_SCALE, Math.max(y, -4) * REPLICA_SCALE * 1.6, z * REPLICA_SCALE);
    {
        const R = small ? 60 : 90;
        const pts = [];
        for (let gz = 0; gz < R; gz++) {
            for (let gx = 0; gx < R; gx++) {
                const x = (gx / (R - 1) - 0.5) * 2 * REPLICA_SPAN, z = (gz / (R - 1) - 0.5) * 2 * REPLICA_SPAN;
                if (x * x + z * z > REPLICA_SPAN * REPLICA_SPAN) continue;
                pts.push(toReplica(x, height(x, z), z));
            }
        }
        const holo = new THREE.Points(
            new THREE.BufferGeometry().setFromPoints(pts),
            new THREE.PointsMaterial({ color: 0xc9a3e6, size: 0.4, map: dot(), opacity: 0.85, ...additive }),
        );
        replica.add(holo);
        const rimRing = new THREE.Line(circle, new THREE.LineBasicMaterial({ color: 0xb04be0, ...additive, opacity: 0.7 }));
        rimRing.scale.setScalar(REPLICA_SPAN * REPLICA_SCALE);
        rimRing.position.y = -0.6;
        replica.add(rimRing);
    }
    const replicaScan = new THREE.Line(circle, new THREE.LineBasicMaterial({ color: 0xf0d8ff, ...additive, opacity: 0.5 }));
    replica.add(replicaScan);
    const replicaMark = new THREE.Line(circle, new THREE.LineBasicMaterial({ color: 0xff9ad5, ...additive, opacity: 0 }));
    replica.add(replicaMark);
    const beam = new THREE.Mesh(
        new THREE.CylinderGeometry(0.2, 2.2, REPLICA_Y - 2, 16, 1, true),
        new THREE.MeshBasicMaterial({ color: 0xb04be0, ...additive, opacity: 0.12, side: THREE.DoubleSide }),
    );
    beam.position.y = (REPLICA_Y - 2) / 2 + 2;
    scene.add(beam);

    /* Robots, drones, boats and operators from the generated models. */
    const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
    const load = (name) => loader.loadAsync(`/assets/models/${name}.glb`).then((g) => g.scene);
    const [droneSrc, quadSrc, roverSrc, boatSrc, operatorSrc] = await Promise.all(
        ['drone', 'quadruped', 'rover', 'boat', 'operator'].map(load),
    );
    const sizes = { drone: 7, quadruped: 6.5, rover: 7, boat: 13, operator: 5.2 };
    // Models face +z when they move. The generated quadruped and rover face -z, so they turn around.
    function prepare(src, size, useHeight = false, flip = false) {
        src.rotation.y = flip ? Math.PI : 0;
        const box = new THREE.Box3().setFromObject(src);
        const dims = box.getSize(new THREE.Vector3());
        src.scale.setScalar(size / (useHeight ? dims.y : Math.max(dims.x, dims.z)));
        const scaled = new THREE.Box3().setFromObject(src);
        const center = scaled.getCenter(new THREE.Vector3());
        src.position.set(-center.x, -scaled.min.y, -center.z);
        src.traverse((o) => {
            if (!o.isMesh) return;
            o.material = o.material.clone();
            o.material.emissive = new THREE.Color(0x2a1240);
            o.material.emissiveIntensity = 1;
        });
        const holder = new THREE.Group();
        holder.add(src);
        holder.userData.box = new THREE.Box3().setFromObject(holder).getSize(new THREE.Vector3());
        return holder;
    }
    const templates = {
        drone: prepare(droneSrc, sizes.drone),
        quadruped: prepare(quadSrc, sizes.quadruped, false, true),
        rover: prepare(roverSrc, sizes.rover, false, true),
        boat: prepare(boatSrc, sizes.boat),
        operator: prepare(operatorSrc, sizes.operator, true),
    };

    const glowTexture = radialTexture('rgba(210, 150, 255, 0.9)', 'rgba(120, 40, 180, 0)');
    const glowMaterial = new THREE.MeshBasicMaterial({ map: glowTexture, ...additive, opacity: 0.45 });
    const glowDisc = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    const coneMaterial = new THREE.MeshBasicMaterial({ color: 0xb04be0, ...additive, opacity: 0.06, side: THREE.DoubleSide });
    const scanMaterial = new THREE.LineBasicMaterial({ color: 0xe9d5ff, ...additive, opacity: 0.55 });
    const frustumMaterial = new THREE.LineBasicMaterial({ color: 0x9ad8ff, ...additive, opacity: 0.45 });
    // Each RGB-D camera sits where the model carries it, as a fraction of the model's
    // width, height and length: the quadruped's head and the top of the rover's mast.
    const cameraMounts = { quadruped: [0, 0.92, 0.42], rover: [0, 0.95, 0.12] };
    function frustumGeometry(kind) {
        const box = templates[kind].userData.box;
        const [mx, my, mz] = cameraMounts[kind];
        const o = new THREE.Vector3(mx * box.x, my * box.y, mz * box.z);
        // A 16:9 far plane, 9 by 5.06, 11 ahead and tilted down toward the ground.
        const far = [[-4.5, 2.53], [4.5, 2.53], [4.5, -2.53], [-4.5, -2.53]]
            .map(([x, y]) => new THREE.Vector3(o.x + x, o.y - 4.5 + y, o.z + 11));
        const pts = [];
        far.forEach((c, i) => pts.push(o, c, c, far[(i + 1) % 4]));
        return new THREE.BufferGeometry().setFromPoints(pts);
    }
    const frusta = { quadruped: frustumGeometry('quadruped'), rover: frustumGeometry('rover') };

    const agents = [];
    const names = { drone: 0, quadruped: 0, rover: 0, boat: 0, operator: 0 };
    function randomPoint(minR, maxR) {
        for (;;) {
            const r = minR + Math.random() * (maxR - minR);
            const t = Math.random() * Math.PI * 2;
            const p = new THREE.Vector3(Math.cos(t) * r, 0, Math.sin(t) * r);
            if (onLand(p.x, p.z)) return p;
        }
    }

    function addAgent(kind, options = {}) {
        const mesh = templates[kind].clone();
        scene.add(mesh);
        const start = options.start || randomPoint(40, 125);
        const agent = {
            kind, mesh,
            name: `${kind}-${++names[kind]}`,
            pos: new THREE.Vector3(start.x, height(start.x, start.z), start.z),
            vel: new THREE.Vector3(0, 0, 1),
            target: randomPoint(40, 135),
            speed: 3,
            alt: 0,
            phase: Math.random() * 100,
            ...options,
        };
        agent.baseSpeed = agent.speed;
        if (kind !== 'drone') {
            agent.glow = new THREE.Mesh(glowDisc, glowMaterial);
            agent.glow.scale.setScalar(kind === 'boat' ? 16 : 10);
            scene.add(agent.glow);
        }
        if (kind === 'drone') {
            agent.cone = new THREE.Mesh(new THREE.ConeGeometry(1, 1, 28, 1, true), coneMaterial);
            agent.scan = new THREE.Line(circle, scanMaterial);
            scene.add(agent.cone, agent.scan);
        }
        if (kind === 'quadruped' || kind === 'rover') {
            agent.mesh.add(new THREE.LineSegments(frusta[kind], frustumMaterial));
        }
        agents.push(agent);
        return agent;
    }

    const counts = small
        ? { drone: 4, quadruped: 2, rover: 2, boat: 1, operator: 2 }
        : { drone: 7, quadruped: 4, rover: 4, boat: 2, operator: 3 };
    for (let i = 0; i < counts.drone; i++) addAgent('drone', { speed: 8 + Math.random() * 5, alt: 16 + Math.random() * 10 });
    for (let i = 0; i < counts.quadruped; i++) addAgent('quadruped', { speed: 2.4 + Math.random() });
    for (let i = 0; i < counts.rover; i++) addAgent('rover', { speed: 3.5 + Math.random() * 1.5 });
    for (let i = 0; i < counts.boat; i++) addAgent('boat', { canal: true, u: Math.random() * 400 - 200, speed: 4 + Math.random() * 2 });
    [[-14, 10], [12, -12], [18, 14]].slice(0, counts.operator).forEach(([x, z]) =>
        addAgent('operator', { still: true, start: new THREE.Vector3(x, 0, z) }));

    /* Comms: links to the nearest tower, mesh links between neighbours, and packets. */
    const MAX_LINKS = 128;
    function segmentBatch(color, opacity) {
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.BufferAttribute(new Float32Array(MAX_LINKS * 6), 3));
        const line = new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ color, ...additive, opacity }));
        line.frustumCulled = false;
        scene.add(line);
        return { line, array: geometry.attributes.position.array, n: 0 };
    }
    const upLinks = segmentBatch(0xb04be0, 0.3);
    const weakLinks = segmentBatch(0xff7ab6, 0.3);
    const meshLinks = segmentBatch(0x9ad8ff, 0.18);
    const pushSegment = (batch, a, b) => {
        if (batch.n >= MAX_LINKS) return;
        batch.array.set([a.x, a.y, a.z, b.x, b.y, b.z], batch.n * 6);
        batch.n++;
    };
    const flush = (batch) => {
        batch.line.geometry.setDrawRange(0, batch.n * 2);
        batch.line.geometry.attributes.position.needsUpdate = true;
        batch.n = 0;
    };

    const MAX_PACKETS = 160;
    const packetPositions = new Float32Array(MAX_PACKETS * 3);
    const packetColors = new Float32Array(MAX_PACKETS * 3);
    const packetGeometry = new THREE.BufferGeometry();
    packetGeometry.setAttribute('position', new THREE.BufferAttribute(packetPositions, 3));
    packetGeometry.setAttribute('color', new THREE.BufferAttribute(packetColors, 3));
    const packetPoints = new THREE.Points(packetGeometry, new THREE.PointsMaterial({ size: 2.2, map: dot(), vertexColors: true, ...additive }));
    packetPoints.frustumCulled = false;
    scene.add(packetPoints);
    const packets = [];
    const replicaCenter = new THREE.Vector3(0, REPLICA_Y, 0);
    function sendPacket(path, down = false, speed = 40) {
        if (packets.length >= MAX_PACKETS) return;
        let length = 0;
        for (let i = 1; i < path.length; i++) length += path[i].distanceTo(path[i - 1]);
        packets.push({ path: path.map((p) => p.clone()), length, d: 0, speed, down });
    }

    function nearestTower(p) {
        let best = null, bestD = Infinity;
        for (const t of towers) {
            const d = Math.hypot(t.top.x - p.x, t.top.z - p.z);
            if (d < bestD) { bestD = d; best = t; }
        }
        return { tower: best, distance: bestD };
    }

    /* Detection boxes with labels. */
    const boxEdges = new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1));
    const detections = [];
    function detect(target, label, life = 2.6) {
        const box = new THREE.LineSegments(boxEdges, new THREE.LineBasicMaterial({ color: 0xff9ad5, ...additive, opacity: 0.9 }));
        const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: labelTexture(label), transparent: true, depthWrite: false }));
        sprite.scale.set(9, 1.7, 1);
        scene.add(box, sprite);
        const detection = { target, box, sprite, age: 0, life };
        detections.push(detection);
        return detection;
    }
    function relabel(detection, label) {
        detection.sprite.material.map.dispose();
        detection.sprite.material.map = labelTexture(label);
    }

    /* The anomaly the event loop investigates. */
    const anomaly = new THREE.Mesh(
        new THREE.OctahedronGeometry(1.4, 0),
        new THREE.MeshBasicMaterial({ color: 0xff9ad5, wireframe: true, transparent: true, opacity: 0 }),
    );
    scene.add(anomaly);
    const route = new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3()]),
        new THREE.LineDashedMaterial({ color: 0xff9ad5, dashSize: 1.4, gapSize: 1, ...additive, opacity: 0 }));
    route.frustumCulled = false;
    scene.add(route);

    const hudEvent = document.querySelector('[data-hud="event"]');
    const setEvent = (text) => { if (hudEvent) hudEvent.textContent = text; };
    const event = { phase: 'idle', next: 6, t0: 0, point: new THREE.Vector3(), drone: null, quad: null, detection: null };

    function stepEvent(time) {
        if (event.phase === 'idle') {
            if (time < event.next) return;
            // Pick a spot the viewer can see: mid distance, right of the page text, above its lower edge.
            const ndc = new THREE.Vector3();
            let chosen = randomPoint(45, 110);
            for (let tries = 0; tries < 40; tries++) {
                const p = randomPoint(45, 120);
                p.y = height(p.x, p.z);
                const range = p.distanceTo(camera.position);
                ndc.copy(p).project(camera);
                if (range > 70 && range < 220 && ndc.x > 0.05 && ndc.x < 0.9 && ndc.y > -0.3 && ndc.y < 0.6) { chosen = p; break; }
            }
            event.point.copy(chosen);
            event.point.y = height(event.point.x, event.point.z) + 2;
            anomaly.position.copy(event.point);
            anomaly.material.opacity = 0.9;
            u.uEvent.value.set(event.point.x, event.point.z, time, 1);
            event.drone = agents.filter((a) => a.kind === 'drone').sort((a, b) => a.pos.distanceTo(event.point) - b.pos.distanceTo(event.point))[0];
            event.drone.override = event.point.clone();
            event.phase = 'investigate';
            event.t0 = time;
            setEvent('anomaly reported');
        } else if (event.phase === 'investigate') {
            const d = Math.hypot(event.drone.pos.x - event.point.x, event.drone.pos.z - event.point.z);
            if (d < 8 || time - event.t0 > 14) {
                event.detection = detect(anomaly, 'unknown 0.62', 999);
                const { tower } = nearestTower(event.drone.pos);
                for (let k = 0; k < 5; k++) {
                    setTimeout(() => sendPacket([event.drone.pos, tower.top, replicaCenter], false, 55), k * 120);
                }
                event.phase = 'model';
                event.t0 = time;
                setEvent(`detected by ${event.drone.name}`);
            }
        } else if (event.phase === 'model') {
            if (time - event.t0 > 1.8) {
                toReplica(event.point.x, event.point.y, event.point.z, replicaMark.position);
                replicaMark.material.opacity = 1;
                event.quad = agents.filter((a) => a.kind === 'quadruped').sort((a, b) => a.pos.distanceTo(event.point) - b.pos.distanceTo(event.point))[0];
                event.quad.override = event.point.clone();
                event.quad.speed = 6.5;
                const { tower } = nearestTower(event.quad.pos);
                sendPacket([replicaCenter, tower.top, event.quad.pos], true, 60);
                route.material.opacity = 0.8;
                event.phase = 'dispatch';
                event.t0 = time;
                setEvent('world model updated');
                setTimeout(() => setEvent(`dispatched ${event.quad.name}`), 1200);
            }
        } else if (event.phase === 'dispatch') {
            const q = event.quad.pos;
            const pts = route.geometry.attributes.position.array;
            pts.set([q.x, q.y + 1, q.z, event.point.x, event.point.y - 1, event.point.z]);
            route.geometry.attributes.position.needsUpdate = true;
            route.computeLineDistances();
            const d = Math.hypot(q.x - event.point.x, q.z - event.point.z);
            if (d < 6 || time - event.t0 > 30) {
                relabel(event.detection, 'cleared');
                event.detection.life = event.detection.age + 2.2;
                setEvent(`resolved in ${Math.round(time - u.uEvent.value.z)} s`);
                event.phase = 'resolve';
                event.t0 = time;
            }
        } else if (event.phase === 'resolve') {
            const k = Math.max(0, 1 - (time - event.t0) / 2);
            anomaly.material.opacity = 0.9 * k;
            replicaMark.material.opacity = k;
            route.material.opacity = 0.8 * k;
            if (k === 0) {
                u.uEvent.value.w = 0;
                event.drone.override = null;
                event.quad.override = null;
                event.quad.speed = event.quad.baseSpeed;
                event.phase = 'idle';
                event.next = time + 9;
                setEvent('watching');
            }
        }
    }

    /* Agent motion. */
    const tmp = new THREE.Vector3();
    function stepAgent(agent, dt, time) {
        if (agent.still) {
            agent.pos.y = height(agent.pos.x, agent.pos.z);
            agent.mesh.rotation.y = Math.sin(time * 0.2 + agent.phase) * 1.2;
        } else if (agent.canal) {
            agent.u += agent.speed * dt;
            if (agent.u > 230) agent.u = -230;
            const x = agent.u, z = canalZ(x);
            agent.pos.set(x, height(x, z) + 0.2 + Math.sin(time * 1.6 + agent.phase) * 0.12, z);
            agent.mesh.rotation.y = Math.atan2(1, canalZ(x + 1) - z);
        } else {
            const goal = agent.override || agent.target;
            tmp.copy(goal).sub(agent.pos);
            tmp.y = 0;
            if (!agent.override && tmp.length() < 6) agent.target = randomPoint(40, 140);
            const arrive = agent.override ? Math.min(1, tmp.length() / 8) : 1;
            tmp.normalize().multiplyScalar(agent.speed * arrive);
            agent.vel.lerp(tmp, Math.min(1, dt * 1.2));
            // A soft push keeps everything out of the column under the world model.
            const r = Math.hypot(agent.pos.x, agent.pos.z);
            if (r < 34 && !agent.override) {
                const push = (34 - r) / 34;
                agent.vel.x += (agent.pos.x / Math.max(r, 0.1)) * push * 12 * dt;
                agent.vel.z += (agent.pos.z / Math.max(r, 0.1)) * push * 12 * dt;
            }
            agent.pos.x += agent.vel.x * dt;
            agent.pos.z += agent.vel.z * dt;
            const ground = height(agent.pos.x, agent.pos.z);
            if (agent.kind === 'drone') {
                agent.pos.y += (ground + agent.alt - agent.pos.y) * Math.min(1, dt * 1.4);
                agent.mesh.rotation.z = -agent.vel.x * 0.02;
                agent.mesh.rotation.x = agent.vel.z * 0.02;
            } else {
                const bob = agent.kind === 'quadruped' ? Math.abs(Math.sin(time * 9 + agent.phase)) * 0.12 : 0;
                agent.pos.y = ground + bob;
            }
            if (agent.vel.lengthSq() > 0.01) agent.mesh.rotation.y = Math.atan2(agent.vel.x, agent.vel.z);
        }
        agent.mesh.position.copy(agent.pos);
        if (agent.glow) agent.glow.position.set(agent.pos.x, height(agent.pos.x, agent.pos.z) + 0.15, agent.pos.z);

        if (agent.kind === 'drone') {
            const ground = height(agent.pos.x, agent.pos.z);
            const h = Math.max(1, agent.pos.y - ground);
            agent.cone.scale.set(h * 0.32, h, h * 0.32);
            agent.cone.position.set(agent.pos.x, ground + h / 2, agent.pos.z);
            const s = (time * 0.7 + agent.phase) % 1;
            agent.scan.position.set(agent.pos.x, agent.pos.y - s * h, agent.pos.z);
            agent.scan.scale.setScalar(Math.max(0.05, s * h * 0.32));
            agent.scan.material.opacity = 0.55 * (1 - s);
        }

        // Comms: every agent links to its nearest tower; out of range it falls back to a weak link.
        if (agent.kind !== 'operator') {
            const { tower, distance } = nearestTower(agent.pos);
            const from = tmp.set(agent.pos.x, agent.pos.y + 1.5, agent.pos.z);
            agent.inRange = distance < TOWER_RANGE;
            if (agent.inRange) pushSegment(upLinks, from, tower.top);
            else if (Math.sin(time * 23 + agent.phase) > -0.2) pushSegment(weakLinks, from, tower.top);
            agent.tower = tower;
        }
    }

    /* Camera stops follow the page sections. Text-heavy sections keep a wide view. */
    // Stops are set by azimuth, distance and height. The azimuth only increases, so
    // scrolling down turns the world one way and scrolling up turns it back.
    const stops = [
        { id: 'top', az: 50, radius: 125, height: 58, look: [0, 34, 0] },
        { id: 'capabilities', az: 62, radius: 200, height: 110, look: [0, 10, 0] },
        { id: 'use-cases', az: 74, radius: 150, height: 60, look: [0, 6, 0] },
        { id: 'compare', az: 86, radius: 190, height: 120, look: [0, 4, 0] },
        { id: 'start', az: 98, radius: 210, height: 150, look: [0, 0, 0] },
    ].map((s) => ({ ...s, az: THREE.MathUtils.degToRad(s.az), look: new THREE.Vector3(...s.look), y: 0 }));

    function measureStops() {
        const max = Math.max(1, document.documentElement.scrollHeight - window.innerHeight);
        for (const stop of stops) {
            const el = stop.id === 'top' ? null : document.getElementById(stop.id);
            stop.y = el ? Math.min(max, el.getBoundingClientRect().top + window.scrollY - window.innerHeight * 0.35) : 0;
        }
    }

    const pointer = { x: 0, y: 0, tx: 0, ty: 0 };
    window.addEventListener('pointermove', (e) => {
        pointer.tx = e.clientX / window.innerWidth - 0.5;
        pointer.ty = e.clientY / window.innerHeight - 0.5;
    }, { passive: true });

    function resize() {
        const w = window.innerWidth, h = window.innerHeight;
        renderer.setSize(w, h, false);
        camera.aspect = w / h;
        if (w > 960) camera.setViewOffset(w, h, -w * 0.16, 0, w, h);
        else camera.clearViewOffset();
        camera.updateProjectionMatrix();
        measureStops();
    }
    window.addEventListener('resize', resize);
    window.addEventListener('load', measureStops);
    resize();

    const camPos = new THREE.Vector3(), camLook = new THREE.Vector3();
    const view = { az: stops[0].az, radius: stops[0].radius, height: stops[0].height };
    function cameraFor(scrollY) {
        let i = 0;
        while (i < stops.length - 2 && scrollY > stops[i + 1].y) i++;
        const a = stops[i], b = stops[i + 1];
        let t = Math.min(1, Math.max(0, (scrollY - a.y) / Math.max(1, b.y - a.y)));
        t = t * t * (3 - 2 * t);
        view.az = a.az + (b.az - a.az) * t;
        view.radius = a.radius + (b.radius - a.radius) * t;
        view.height = a.height + (b.height - a.height) * t;
        camLook.lerpVectors(a.look, b.look, t);
    }
    cameraFor(0);
    const smoothPos = new THREE.Vector3(Math.cos(view.az) * view.radius, view.height, Math.sin(view.az) * view.radius);
    const smoothLook = camLook.clone();

    const hud = {
        time: document.querySelector('[data-hud="time"]'),
        agents: document.querySelector('[data-hud="agents"]'),
        points: document.querySelector('[data-hud="points"]'),
        rate: document.querySelector('[data-hud="rate"]'),
    };
    if (hud.agents) hud.agents.textContent = String(agents.length);
    if (hud.points) hud.points.textContent = count.toLocaleString('en-US');
    setEvent('watching');

    let last = performance.now();
    let time = 0, frames = 0, lastHud = 0, running = true, packetClock = 0;

    function frame() {
        if (!running) return;
        const now = performance.now();
        const dt = Math.min((now - last) / 1000, 0.05);
        last = now;
        time += dt;
        frames++;

        agents.forEach((agent) => stepAgent(agent, dt, time));
        stepEvent(time);

        // Terrain uniforms from the agents.
        let d = 0, g = 0, f = 0, l = 0;
        for (const agent of agents) {
            if (agent.kind === 'drone' && d < MAX_DRONES) {
                u.uDrones.value[d++].set(agent.pos.x, agent.pos.z, 30 + agent.alt * 1.6, 1);
            } else if (agent.kind !== 'drone' && g < MAX_GLOWS) {
                u.uGlows.value[g++].set(agent.pos.x, agent.pos.z, 0, agent.kind === 'boat' ? 0 : 1);
            }
            if ((agent.kind === 'quadruped' || agent.kind === 'rover') && f < MAX_FRUSTA) {
                u.uFrusta.value[f++].set(agent.pos.x, agent.pos.z, agent.mesh.rotation.y, 1);
            }
            if (agent.kind === 'rover' && l < MAX_LIDAR) {
                u.uLidar.value[l++].set(agent.pos.x, agent.pos.z, time * 2.6 + agent.phase, 1);
            }
        }
        u.uTime.value = time;

        // Mesh links between neighbours on the ground.
        for (let i = 0; i < agents.length; i++) {
            for (let j = i + 1; j < agents.length; j++) {
                const a = agents[i], b = agents[j];
                if (a.kind === 'operator' || b.kind === 'operator') continue;
                if (a.pos.distanceToSquared(b.pos) < 30 * 30) {
                    pushSegment(meshLinks, tmp.set(a.pos.x, a.pos.y + 1.2, a.pos.z), b.pos.clone().setY(b.pos.y + 1.2));
                }
            }
        }
        for (const t of towers) pushSegment(upLinks, t.top, replicaCenter);
        flush(upLinks);
        flush(weakLinks);
        flush(meshLinks);
        upLinks.line.material.opacity = 0.2 + 0.1 * (0.5 + 0.5 * Math.sin(time * 1.7));
        weakLinks.line.material.opacity = 0.15 + 0.25 * Math.random();

        // Packets: uplinks from robots in range, and occasional downlinks from the model.
        packetClock += dt;
        while (packetClock > 0.18) {
            packetClock -= 0.18;
            const movers = agents.filter((a) => a.inRange);
            if (movers.length) {
                const a = movers[Math.floor(Math.random() * movers.length)];
                if (Math.random() < 0.25) sendPacket([replicaCenter, a.tower.top, a.pos], true);
                else sendPacket([a.pos, a.tower.top, replicaCenter]);
            }
        }
        for (let i = packets.length - 1; i >= 0; i--) {
            const p = packets[i];
            p.d += p.speed * dt;
            if (p.d >= p.length) packets.splice(i, 1);
        }
        packets.forEach((p, i) => {
            let rest = p.d;
            let k = 1;
            while (k < p.path.length - 1 && rest > p.path[k].distanceTo(p.path[k - 1])) {
                rest -= p.path[k].distanceTo(p.path[k - 1]);
                k++;
            }
            const a = p.path[k - 1], b = p.path[k];
            tmp.lerpVectors(a, b, Math.min(1, rest / Math.max(0.001, a.distanceTo(b))));
            packetPositions.set([tmp.x, tmp.y, tmp.z], i * 3);
            packetColors.set(p.down ? [0.6, 0.9, 1.0] : [0.95, 0.75, 1.0], i * 3);
        });
        packetGeometry.setDrawRange(0, packets.length);
        packetGeometry.attributes.position.needsUpdate = true;
        packetGeometry.attributes.color.needsUpdate = true;

        // Towers pulse their RF rings.
        towers.forEach((t, k) => {
            t.rings.forEach((ring, r) => {
                const s = ((time * 0.45 + r / 3 + k * 0.17) % 1);
                ring.scale.setScalar(1 + s * 26);
                ring.material.opacity = 0.55 * (1 - s);
            });
            t.beacon.scale.setScalar(1 + Math.sin(time * 5 + k) * 0.3);
        });

        // Replica scan ring and event mark.
        const scan = (time * 0.25) % 1;
        replicaScan.scale.setScalar(scan * REPLICA_SPAN * REPLICA_SCALE);
        replicaScan.material.opacity = 0.5 * (1 - scan);
        replicaMark.scale.setScalar(0.6 + 0.4 * Math.sin(time * 6));
        anomaly.rotation.y = time * 1.5;

        // Detection boxes follow their targets and fade out.
        for (let i = detections.length - 1; i >= 0; i--) {
            const det = detections[i];
            det.age += dt;
            const target = det.target;
            const box = target.userData?.box || (target.mesh && target.mesh.userData.box) || new THREE.Vector3(3, 3, 3);
            const at = target.mesh ? target.mesh.position : target.position;
            det.box.scale.set(box.x * 1.15 + 0.5, box.y * 1.15 + 0.5, box.z * 1.15 + 0.5);
            det.box.position.set(at.x, at.y + box.y / 2 - (target.mesh ? 0 : box.y / 2), at.z);
            det.sprite.position.set(at.x, at.y + box.y + 2.4, at.z);
            const fade = Math.min(1, (det.life - det.age) / 0.6);
            det.box.material.opacity = 0.9 * fade;
            det.sprite.material.opacity = fade;
            if (det.age > det.life) {
                scene.remove(det.box, det.sprite);
                det.sprite.material.map.dispose();
                detections.splice(i, 1);
            }
        }
        // Robots occasionally recognise each other and the operators.
        if (Math.random() < dt / 3.5 && detections.length < 3) {
            const seer = agents[Math.floor(Math.random() * agents.length)];
            const ndc = new THREE.Vector3();
            const seen = agents.filter((a) => {
                if (a === seer || a.kind === 'drone' || a.pos.distanceTo(seer.pos) > 45) return false;
                const range = a.pos.distanceTo(camera.position);
                ndc.copy(a.pos).project(camera);
                // Keep labels readable and clear of the page text: mid distance, upper screen, right side.
                return range > 70 && range < 260 && ndc.y > -0.35 && ndc.x > -0.2 && Math.abs(ndc.x) < 0.95;
            });
            if (seen.length) {
                const t = seen[Math.floor(Math.random() * seen.length)];
                const score = (0.9 + Math.random() * 0.09).toFixed(2);
                detect(t, `${t.kind === 'operator' ? 'person' : t.kind} ${score}`);
            }
        }

        // Camera and veil.
        // The camera follows the scroll position directly, through one short
        // frame-rate independent smoothing step that absorbs jitter without lagging.
        cameraFor(window.scrollY);
        pointer.x += (pointer.tx - pointer.x) * 0.04;
        pointer.y += (pointer.ty - pointer.y) * 0.04;
        const az = view.az + pointer.x * 0.06;
        camPos.set(Math.cos(az) * view.radius, view.height - pointer.y * 6, Math.sin(az) * view.radius);
        const follow = 1 - Math.exp(-dt * 8);
        smoothPos.lerp(camPos, follow);
        smoothLook.lerp(camLook, follow);
        camera.position.copy(smoothPos);
        camera.lookAt(smoothLook);
        veil.style.opacity = String(Math.min(1, window.scrollY / (window.innerHeight * 0.7)));

        renderer.render(scene, camera);

        if (time - lastHud > 0.25) {
            if (hud.time) {
                const m = Math.floor(time / 60);
                hud.time.textContent = `${String(m).padStart(2, '0')}:${(time % 60).toFixed(1).padStart(4, '0')}`;
            }
            if (hud.rate) hud.rate.textContent = `${Math.round(frames / (time - lastHud))} Hz`;
            frames = 0;
            lastHud = time;
        }
        requestAnimationFrame(frame);
    }

    document.addEventListener('visibilitychange', () => {
        if (!document.hidden && !running) {
            running = true;
            last = performance.now();
            requestAnimationFrame(frame);
        } else if (document.hidden) {
            running = false;
        }
    });

    document.documentElement.classList.add('has-world');
    requestAnimationFrame(frame);
}

if (canvas && hasWebGL()) start();
else if (canvas) canvas.remove();
