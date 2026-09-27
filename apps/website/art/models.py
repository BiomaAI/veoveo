"""Generate the robot, vehicle and operator models in the veoveo.ai 3D scene.

Each model starts as a reference still from openai/gpt-image-2 in the site's graphite
and violet style. WaveSpeed Trellis 2 turns the still into a textured glTF:

    uv run --env-file ../../.env --python 3.13 art/models.py still [name ...]
    uv run --env-file ../../.env --python 3.13 art/models.py model [name ...]

The boat uses Hunyuan3D v3 with an extra rear view because a single front view leaves
its stern undefined. Compress every model before copying it to public/assets/models/:

    npx @gltf-transform/cli@4.5.0 optimize art/out/models/NAME.glb public/assets/models/NAME.glb \
      --compress meshopt --texture-compress webp --texture-size 256 \
      --simplify true --simplify-ratio 0.08 --simplify-error 0.01
"""
import json, os, subprocess, sys, tempfile, time, urllib.request
from concurrent.futures import ThreadPoolExecutor
KEY = os.environ["MEDIA_PROVIDER_API_KEY"]; OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "out", "models")
os.makedirs(OUT, exist_ok=True)
def api(path, payload=None):
    req = urllib.request.Request("https://api.wavespeed.ai" + path,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
        method="POST" if payload is not None else "GET")
    with urllib.request.urlopen(req, timeout=180) as r: return json.loads(r.read())
def wait(pid, limit=1800):
    end = time.time() + limit
    while time.time() < end:
        time.sleep(8); r = api(f"/api/v3/predictions/{pid}/result")["data"]
        if r["status"] == "completed": return r["outputs"][0]
        if r["status"] == "failed": raise SystemExit(f"failed: {r.get('error')}")
    raise SystemExit("timed out")
def upload(path):
    jpg = tempfile.NamedTemporaryFile(suffix=".jpg", delete=False).name
    subprocess.run(["sips", "-s", "format", "jpeg", "-s", "formatOptions", "95", path, "--out", jpg], check=True, capture_output=True)
    body = open(jpg, "rb").read()
    for attempt in range(5):
        try:
            b = f"----v{int(time.time()*1000)}"
            data = (f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="a.jpg"\r\nContent-Type: image/jpeg\r\n\r\n').encode() + body + f"\r\n--{b}--\r\n".encode()
            req = urllib.request.Request("https://api.wavespeed.ai/api/v3/media/upload/binary", data=data, method="POST",
                headers={"Authorization": f"Bearer {KEY}", "Content-Type": f"multipart/form-data; boundary={b}"})
            return json.loads(urllib.request.urlopen(req, timeout=180).read())["data"]["download_url"]
        except OSError as e:
            print("upload retry", e, flush=True); time.sleep(4 * (attempt + 1))
    raise SystemExit("upload failed")
STYLE = (" Single isolated object centered on a plain pure white background, full object visible with margin,"
         " three-quarter front view from slightly above, soft even studio lighting, no shadows on the floor,"
         " no background elements, no text, no logos. Sleek matte graphite and dark gray materials with a few"
         " small glowing violet accent lights, hex B04BE0. Clean modern industrial design, realistic proportions,"
         " product-render quality.")
ASSETS = {
  "quadruped": "A four-legged robot dog for industrial inspection, like a modern legged robot, with a compact sensor head, standing on all four legs." + STYLE,
  "rover": "A compact four-wheeled autonomous ground rover robot with a low chassis, rugged wheels and a small sensor mast." + STYLE,
  "drone": "A quadcopter survey drone with four rotors on arms and a small gimbal camera underneath, seen hovering." + STYLE,
  "boat": "A small uncrewed autonomous surface vessel, a sleek robotic boat about three meters long with a sensor mast, no people on board." + STYLE,
  "operator": "A field operator engineer standing and holding a rugged tablet, wearing a dark technical jacket, trousers and work boots, neutral pose, full body." + STYLE.replace(" Sleek matte graphite and dark gray materials with a few small glowing violet accent lights, hex B04BE0.", " Dark clothing with one small violet detail on the jacket."),
}
def still(n):
    pid = api("/api/v3/openai/gpt-image-2/text-to-image", {"prompt": ASSETS[n], "aspect_ratio": "1:1", "resolution": "2k", "quality": "high", "output_format": "png"})["data"]["id"]
    urllib.request.urlretrieve(wait(pid, 900), f"{OUT}/{n}.png"); return f"still {n}"
def model(n):
    pid = api("/api/v3/wavespeed-ai/trellis-2/image-to-3d", {"image": upload(f"{OUT}/{n}.png"), "resolution": "1024", "target_face_count": 60000, "texture_size": 2048, "remove_background": True})["data"]["id"]
    url = wait(pid, 2400); urllib.request.urlretrieve(url, f"{OUT}/{n}.glb"); return f"model {n} {os.path.getsize(f'{OUT}/{n}.glb')//1024} KB"
if __name__ == "__main__":
    mode, names = sys.argv[1], sys.argv[2:] or list(ASSETS)
    with ThreadPoolExecutor(5) as ex:
        for r in ex.map({"still": still, "model": model}[mode], names): print(r, flush=True)
