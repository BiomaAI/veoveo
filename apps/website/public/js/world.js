// Live world behind the page: a point-cloud terrain with radiating scan pulses, a floating
// world model, and robots that move through it. Page scroll moves the camera between stops.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';

const canvas = document.getElementById('world');
const small = window.matchMedia('(max-width: 760px)').matches;

function hasWebGL() {
    try {
        const test = document.createElement('canvas');
        return !!test.getContext('webgl2');
    } catch {
        return false;
    }
}

if (canvas && hasWebGL()) start();
else if (canvas) canvas.remove();

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

// The canal the boats follow: a gentle curve crossing the valley.
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
    // A flat plateau at the center where the operation works.
    const plateau = Math.exp(-(x * x + z * z) / 2600);
    h = h * (1 - plateau) + 2 * plateau;
    // The canal cuts a shallow channel.
    const canal = Math.exp(-Math.pow(z - canalZ(x), 2) / 60);
    return h * (1 - canal) - 4 * canal;
}

/* ---------- scene ---------- */

async function start() {
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5));
    renderer.setClearColor(0x04030a, 1);
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;

    const scene = new THREE.Scene();
    scene.fog = new THREE.FogExp2(0x04030a, 0.0036);
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
    const MAX_SENSORS = 24;

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

    const sensors = Array.from({ length: MAX_SENSORS }, () => new THREE.Vector4(0, 0, 0, 0));
    const terrainMaterial = new THREE.ShaderMaterial({
        transparent: true,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        uniforms: {
            uTime: { value: 0 },
            uPixel: { value: renderer.getPixelRatio() },
            uSensors: { value: sensors },
            uFog: { value: scene.fog.density },
            uDim: { value: 1 },
        },
        vertexShader: `
            uniform float uTime;
            uniform float uPixel;
            uniform float uFog;
            uniform vec4 uSensors[${MAX_SENSORS}];
            attribute float aHeight;
            varying vec3 vColor;
            varying float vAlpha;
            void main() {
                vec4 mv = modelViewMatrix * vec4(position, 1.0);
                float dist = length(position.xz);
                // Three scan pulses radiate from the world model at different speeds.
                float ring = 0.0;
                for (int k = 0; k < 3; k++) {
                    float speed = 38.0 + float(k) * 11.0;
                    float r = mod(uTime * speed + float(k) * 90.0, 300.0);
                    ring += exp(-abs(dist - r) * 0.32) * smoothstep(300.0, 80.0, r);
                }
                // A slow sweep line crosses the valley.
                float sweep = exp(-abs(position.x - (mod(uTime * 22.0, 560.0) - 280.0)) * 0.12) * 0.55;
                float hot = 0.0;
                for (int s = 0; s < ${MAX_SENSORS}; s++) {
                    vec4 sn = uSensors[s];
                    vec2 d = position.xz - sn.xz;
                    hot += exp(-dot(d, d) / max(sn.w, 0.001)) * step(0.001, sn.w);
                }
                hot = clamp(hot, 0.0, 1.0);
                float h = clamp((aHeight + 24.0) / 70.0, 0.0, 1.0);
                vec3 low = vec3(0.20, 0.07, 0.34);
                vec3 high = vec3(0.62, 0.50, 0.86);
                vec3 base = mix(low, high, h) * (0.55 + 0.45 * h);
                vec3 pulse = vec3(0.78, 0.40, 1.0);
                vColor = base + pulse * (ring * 0.9 + sweep * 0.6) + vec3(0.95, 0.80, 1.0) * hot * 0.9;
                float depth = -mv.z;
                float fog = 1.0 - exp(-uFog * uFog * depth * depth);
                vAlpha = (0.7 + hot * 0.3) * (1.0 - fog * 0.9);
                gl_PointSize = (2.0 + hot * 2.6 + ring * 1.4) * uPixel * (240.0 / max(depth, 1.0));
                gl_Position = projectionMatrix * mv;
            }
        `,
        fragmentShader: `
            uniform float uDim;
            varying vec3 vColor;
            varying float vAlpha;
            void main() {
                float a = smoothstep(0.5, 0.08, length(gl_PointCoord - 0.5));
                float alpha = a * vAlpha * uDim;
                gl_FragColor = vec4(vColor * alpha, alpha);
            }
        `,
    });
    scene.add(new THREE.Points(terrainGeometry, terrainMaterial));

    /* The world model: four glass layers floating over the plateau. */
    const worldModel = new THREE.Group();
    worldModel.position.set(0, 26, 0);
    const layerFill = new THREE.MeshBasicMaterial({
        color: 0x8a36b8, transparent: true, opacity: 0.07, side: THREE.DoubleSide,
        depthWrite: false, blending: THREE.AdditiveBlending,
    });
    const layerEdge = new THREE.LineBasicMaterial({ color: 0xc9a3e6, transparent: true, opacity: 0.7 });
    const nodeMaterial = new THREE.PointsMaterial({
        color: 0xe7d2ff, size: 1.1, transparent: true, opacity: 0.9, depthWrite: false,
        blending: THREE.AdditiveBlending,
    });
    const layerGeometry = new THREE.PlaneGeometry(30, 30);
    layerGeometry.rotateX(-Math.PI / 2);
    const layerEdges = new THREE.EdgesGeometry(layerGeometry);
    const layers = [];
    for (let l = 0; l < 4; l++) {
        const layer = new THREE.Group();
        layer.position.y = l * 3.4;
        layer.add(new THREE.Mesh(layerGeometry, layerFill));
        layer.add(new THREE.LineSegments(layerEdges, layerEdge));
        const nodes = new Float32Array(14 * 3);
        for (let k = 0; k < 14; k++) nodes.set([(hash(l, k) - 0.5) * 26, 0.2, (hash(k, l + 7) - 0.5) * 26], k * 3);
        const nodeGeometry = new THREE.BufferGeometry();
        nodeGeometry.setAttribute('position', new THREE.BufferAttribute(nodes, 3));
        layer.add(new THREE.Points(nodeGeometry, nodeMaterial));
        worldModel.add(layer);
        layers.push(layer);
    }
    const coreBeam = new THREE.Mesh(
        new THREE.CylinderGeometry(0.25, 0.25, 32, 8, 1, true),
        new THREE.MeshBasicMaterial({ color: 0xb04be0, transparent: true, opacity: 0.35, blending: THREE.AdditiveBlending, depthWrite: false }),
    );
    coreBeam.position.y = -16;
    worldModel.add(coreBeam);
    scene.add(worldModel);

    /* Robots, drones, boats and operators from the generated models. */
    const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
    const load = (name) => loader.loadAsync(`/assets/models/${name}.glb`).then((g) => g.scene);
    const [droneSrc, quadSrc, roverSrc, boatSrc, operatorSrc] = await Promise.all(
        ['drone', 'quadruped', 'rover', 'boat', 'operator'].map(load),
    );

    function prepare(src, size, useHeight = false) {
        const box = new THREE.Box3().setFromObject(src);
        const dims = box.getSize(new THREE.Vector3());
        const scale = size / (useHeight ? dims.y : Math.max(dims.x, dims.z));
        src.scale.setScalar(scale);
        const scaled = new THREE.Box3().setFromObject(src);
        const center = scaled.getCenter(new THREE.Vector3());
        src.position.set(-center.x, -scaled.min.y, -center.z);
        const holder = new THREE.Group();
        holder.add(src);
        return holder;
    }
    const templates = {
        drone: prepare(droneSrc, 7),
        quadruped: prepare(quadSrc, 6.5),
        rover: prepare(roverSrc, 7),
        boat: prepare(boatSrc, 13),
        operator: prepare(operatorSrc, 5.2, true),
    };

    for (const holder of Object.values(templates)) {
        holder.traverse((o) => {
            if (!o.isMesh) return;
            o.material = o.material.clone();
            o.material.emissive = new THREE.Color(0x2a1240);
            o.material.emissiveIntensity = 1;
        });
    }

    const coneMaterial = new THREE.MeshBasicMaterial({
        color: 0xb04be0, transparent: true, opacity: 0.1, side: THREE.DoubleSide,
        depthWrite: false, blending: THREE.AdditiveBlending,
    });
    const linkMaterial = new THREE.LineBasicMaterial({
        color: 0xb04be0, transparent: true, opacity: 0.22, blending: THREE.AdditiveBlending, depthWrite: false,
    });
    const TRAIL = 60;

    const agents = [];
    function randomPoint(minR, maxR) {
        const r = minR + Math.random() * (maxR - minR);
        const t = Math.random() * Math.PI * 2;
        return new THREE.Vector3(Math.cos(t) * r, 0, Math.sin(t) * r);
    }

    function addAgent(kind, options = {}) {
        const mesh = templates[kind].clone();
        scene.add(mesh);
        const start = options.start || randomPoint(15, 110);
        const agent = {
            kind, mesh,
            pos: new THREE.Vector3(start.x, height(start.x, start.z), start.z),
            vel: new THREE.Vector3(),
            target: randomPoint(10, 120),
            speed: options.speed || 3,
            alt: options.alt || 0,
            phase: Math.random() * 100,
            ...options,
        };
        const linkGeometry = new THREE.BufferGeometry();
        linkGeometry.setAttribute('position', new THREE.BufferAttribute(new Float32Array(6), 3));
        agent.link = new THREE.Line(linkGeometry, linkMaterial);
        scene.add(agent.link);
        if (kind === 'drone') {
            agent.cone = new THREE.Mesh(new THREE.ConeGeometry(1, 1, 24, 1, true), coneMaterial);
            scene.add(agent.cone);
            agent.trail = new Float32Array(TRAIL * 3);
            for (let k = 0; k < TRAIL; k++) agent.trail.set([agent.pos.x, agent.pos.y + agent.alt, agent.pos.z], k * 3);
            const trailGeometry = new THREE.BufferGeometry();
            trailGeometry.setAttribute('position', new THREE.BufferAttribute(agent.trail, 3));
            const fade = new Float32Array(TRAIL * 3);
            for (let k = 0; k < TRAIL; k++) {
                const f = 1 - k / TRAIL;
                fade.set([0.75 * f, 0.45 * f, 1.0 * f], k * 3);
            }
            trailGeometry.setAttribute('color', new THREE.BufferAttribute(fade, 3));
            agent.trailLine = new THREE.Line(trailGeometry, new THREE.LineBasicMaterial({
                vertexColors: true, transparent: true, opacity: 0.6, blending: THREE.AdditiveBlending, depthWrite: false,
            }));
            scene.add(agent.trailLine);
        }
        agents.push(agent);
        return agent;
    }

    const counts = small
        ? { drone: 4, quadruped: 2, rover: 2, boat: 1, operator: 2 }
        : { drone: 8, quadruped: 4, rover: 4, boat: 2, operator: 3 };
    for (let i = 0; i < counts.drone; i++) addAgent('drone', { speed: 9 + Math.random() * 5, alt: 14 + Math.random() * 10 });
    for (let i = 0; i < counts.quadruped; i++) addAgent('quadruped', { speed: 2.2 + Math.random() });
    for (let i = 0; i < counts.rover; i++) addAgent('rover', { speed: 3.5 + Math.random() * 1.5 });
    for (let i = 0; i < counts.boat; i++) addAgent('boat', { canal: true, u: Math.random() * 400 - 200, speed: 4 + Math.random() * 2 });
    const operatorSpots = [[-14, 10], [12, -12], [18, 14]];
    for (let i = 0; i < counts.operator; i++) {
        const [x, z] = operatorSpots[i];
        addAgent('operator', { still: true, start: new THREE.Vector3(x, 0, z) });
    }

    const tmp = new THREE.Vector3();
    function stepAgent(agent, dt, time) {
        if (agent.still) {
            agent.pos.y = height(agent.pos.x, agent.pos.z);
            agent.mesh.position.copy(agent.pos);
            agent.mesh.rotation.y = Math.sin(time * 0.2 + agent.phase) * 1.2;
        } else if (agent.canal) {
            agent.u += agent.speed * dt;
            if (agent.u > 230) agent.u = -230;
            const x = agent.u, z = canalZ(x);
            const ahead = canalZ(x + 1);
            agent.pos.set(x, height(x, z) + 0.2 + Math.sin(time * 1.6 + agent.phase) * 0.12, z);
            agent.mesh.position.copy(agent.pos);
            agent.mesh.rotation.y = Math.atan2(-(ahead - z), 1) - Math.PI / 2;
        } else {
            tmp.copy(agent.target).sub(agent.pos);
            tmp.y = 0;
            if (tmp.length() < 6) agent.target = randomPoint(10, 125);
            tmp.normalize().multiplyScalar(agent.speed);
            agent.vel.lerp(tmp, Math.min(1, dt * 1.2));
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
            agent.mesh.position.copy(agent.pos);
            agent.mesh.rotation.y = Math.atan2(agent.vel.x, agent.vel.z);
        }

        // Every agent links to the world model; the link brightens with a slow pulse.
        const link = agent.link.geometry.attributes.position.array;
        link.set([agent.pos.x, agent.pos.y + 1.5, agent.pos.z, worldModel.position.x, worldModel.position.y, worldModel.position.z]);
        agent.link.geometry.attributes.position.needsUpdate = true;

        if (agent.kind === 'drone') {
            const ground = height(agent.pos.x, agent.pos.z);
            const h = Math.max(1, agent.pos.y - ground);
            agent.cone.scale.set(h * 0.45, h, h * 0.45);
            agent.cone.position.set(agent.pos.x, ground + h / 2, agent.pos.z);
            agent.trail.copyWithin(3, 0, (TRAIL - 1) * 3);
            agent.trail.set([agent.pos.x, agent.pos.y, agent.pos.z], 0);
            agent.trailLine.geometry.attributes.position.needsUpdate = true;
        }
    }

    /* Camera stops follow the page sections. */
    const stops = [
        { id: 'top', pos: [74, 46, 88], look: [0, 30, 0] },
        { id: 'capabilities', pos: [40, 58, 52], look: [0, 34, 0] },
        { id: 'use-cases', pos: [-58, 16, 40], look: [-4, 5, -4] },
        { id: 'compare', pos: [-54, 84, -104], look: [0, 4, 0] },
        { id: 'start', pos: [0, 118, 150], look: [0, 0, 0] },
    ].map((s) => ({ ...s, pos: new THREE.Vector3(...s.pos), look: new THREE.Vector3(...s.look), y: 0 }));

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
        // On wide screens, the hero text sits on the left, so the scene centers to its right.
        if (w > 960) camera.setViewOffset(w, h, -w * 0.16, 0, w, h);
        else camera.clearViewOffset();
        camera.updateProjectionMatrix();
        measureStops();
    }
    window.addEventListener('resize', resize);
    window.addEventListener('load', measureStops);
    resize();

    const camPos = new THREE.Vector3(), camLook = new THREE.Vector3();
    const smoothPos = stops[0].pos.clone(), smoothLook = stops[0].look.clone();
    function cameraFor(scrollY) {
        let i = 0;
        while (i < stops.length - 2 && scrollY > stops[i + 1].y) i++;
        const a = stops[i], b = stops[i + 1];
        let t = (scrollY - a.y) / Math.max(1, b.y - a.y);
        t = Math.min(1, Math.max(0, t));
        t = t * t * (3 - 2 * t);
        camPos.lerpVectors(a.pos, b.pos, t);
        camLook.lerpVectors(a.look, b.look, t);
    }

    /* Heads-up readout in the hero. */
    const hud = {
        time: document.querySelector('[data-hud="time"]'),
        agents: document.querySelector('[data-hud="agents"]'),
        points: document.querySelector('[data-hud="points"]'),
        rate: document.querySelector('[data-hud="rate"]'),
    };
    if (hud.agents) hud.agents.textContent = String(agents.length);
    if (hud.points) hud.points.textContent = count.toLocaleString('en-US');

    let last = performance.now();
    let time = 0, frames = 0, lastHud = 0, running = true;

    function frame() {
        if (!running) return;
        const now = performance.now();
        const dt = Math.min((now - last) / 1000, 0.05);
        last = now;
        time += dt;
        frames++;

        agents.forEach((agent) => stepAgent(agent, dt, time));
        let s = 0;
        for (const agent of agents) {
            if (s >= MAX_SENSORS) break;
            const radius = agent.kind === 'drone' ? 40 + agent.alt * 2 : agent.kind === 'operator' ? 0 : 14;
            sensors[s++].set(agent.pos.x, 0, agent.pos.z, radius);
        }
        terrainMaterial.uniforms.uTime.value = time;

        worldModel.rotation.y = time * 0.08;
        layers.forEach((layer, l) => { layer.position.y = l * 3.4 + Math.sin(time * 0.9 + l) * 0.35; });
        linkMaterial.opacity = 0.14 + 0.1 * (0.5 + 0.5 * Math.sin(time * 1.7));

        cameraFor(window.scrollY);
        pointer.x += (pointer.tx - pointer.x) * 0.04;
        pointer.y += (pointer.ty - pointer.y) * 0.04;
        const orbit = time * 0.02 + pointer.x * 0.25;
        const cos = Math.cos(orbit), sin = Math.sin(orbit);
        camPos.set(camPos.x * cos - camPos.z * sin, camPos.y - pointer.y * 8, camPos.x * sin + camPos.z * cos);
        smoothPos.lerp(camPos, Math.min(1, dt * 2.5));
        smoothLook.lerp(camLook, Math.min(1, dt * 2.5));
        camera.position.copy(smoothPos);
        camera.lookAt(smoothLook);

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
