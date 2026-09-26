#!/usr/bin/env python3
"""Generate the use-case videos on the veoveo.ai home page through WaveSpeed.

Each use case starts from a still made with openai/gpt-image-2/text-to-image. Seedance 2.5
image-to-video turns the still into an 8-second, three-shot clip from a shot-by-shot prompt.

    uv run --env-file ../../.env --python 3.13 art/usecases.py still [name ...]
    uv run --env-file ../../.env --python 3.13 art/usecases.py video [name ...]

Outputs land in art/out/. Review every clip, then encode it for the page:

    ffmpeg -i art/out/NAME.mp4 -an -vf scale=960:-2 -c:v libx264 -preset slow -crf 26 \
      -pix_fmt yuv420p -movflags +faststart public/assets/cards/NAME.mp4
    ffmpeg -ss 0.3 -i art/out/NAME.mp4 -frames:v 1 -vf scale=960:-2 -q:v 4 public/assets/cards/NAME.jpg
"""
import json, os, subprocess, sys, tempfile, time, urllib.request
from concurrent.futures import ThreadPoolExecutor

KEY = os.environ["MEDIA_PROVIDER_API_KEY"]
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")
IMAGE_MODEL = "openai/gpt-image-2/text-to-image"
VIDEO_MODEL = "bytedance/seedance-2.5/image-to-video"

STILLS = {
    "agriculture": (
        'Long green crop rows at sunrise seen from a low three-quarter angle, with a faint violet wireframe twin of the field overlaid on the rows. A slim four-wheeled field robot straddles one row with its sensors pointed down at the plants. At the edge of the field, a farmer in a cap and jacket stands by a pickup truck with a tablet. Soft golden light, morning haze. Stylized high-quality 3D render in the look of a premium technology keynote, cinematic and photographic. A subtle translucent violet digital-twin overlay (thin wireframe lines, glowing outlines, small data markers, hex 742A98 and B04BE0) sits on top of the real scene. Machines are sleek matte graphite with small violet accent lights. People wear plain work clothes and look natural and focused. Calm, precise, optimistic; no weapons, no text, no labels, no logos, no watermarks, no user interface screens with readable text.'
    ),
    "rescue": (
        'A disaster site at dusk after an earthquake: a partially collapsed concrete building and rubble, seen from an elevated three-quarter view. A tracked crawler robot with headlights waits at the edge of the rubble, a drone hovers above casting a violet wireframe scan over the debris, and three responders in helmets and orange jackets stand at a field table looking at a rugged tablet. Dust in the air, warm sunset light. Stylized high-quality 3D render in the look of a premium technology keynote, cinematic and photographic. A subtle translucent violet digital-twin overlay (thin wireframe lines, glowing outlines, small data markers, hex 742A98 and B04BE0) sits on top of the real scene. Machines are sleek matte graphite with small violet accent lights. People wear plain work clothes and look natural and focused. Calm, precise, optimistic; no weapons, no text, no labels, no logos, no watermarks, no user interface screens with readable text.'
    ),
    "perimeter": (
        'A secure industrial facility at night seen from a high three-quarter aerial view, surrounded by a perimeter fence with slim sensor posts. Each post projects a thin translucent violet wireframe detection fan across the ground, and a faint wireframe grid overlays the terrain like a digital twin. Outside the fence in the dark field, an unidentified object sits inside a glowing violet ring. A sleek four-legged robot with a sensor head trots along a path toward the ring, and a small wheeled ground robot waits by the gate. Floodlit buildings, low fog, dark tree line. Stylized high-quality 3D render in the look of a premium technology keynote: near-black slate and deep navy tones with a soft violet atmospheric glow, matte graphite machines with small violet running lights, hex 742A98 and B04BE0. Calm, precise, forward-looking; no weapons, no text, no labels, no logos, no watermarks, no user interface.'
    ),
    "quality": (
        'A bright modern factory production line seen from a three-quarter elevated view. Metal parts move along a conveyor under a row of inspection cameras. A slim industrial robot arm stands beside the line, and a technician in a work jacket holds a tablet at the side of the conveyor. A faint violet wireframe twin traces the conveyor and each part. Clean light-gray and white factory interior with warm overhead lights. Stylized high-quality 3D render in the look of a premium technology keynote, cinematic and photographic. A subtle translucent violet digital-twin overlay (thin wireframe lines, glowing outlines, small data markers, hex 742A98 and B04BE0) sits on top of the real scene. Machines are sleek matte graphite with small violet accent lights. People wear plain work clothes and look natural and focused. Calm, precise, optimistic; no weapons, no text, no labels, no logos, no watermarks, no user interface screens with readable text.'
    ),
    "inspection": (
        'An electrical substation at night with transformers, insulators and steel frames, seen from a low three-quarter angle along a gravel path. A sleek four-legged inspection robot with a sensor mast walks the path. A faint violet wireframe twin outlines the equipment. At the edge of the frame, an engineer in a hard hat stands by a service truck holding a tablet. Sodium lights, light mist. Stylized high-quality 3D render in the look of a premium technology keynote, cinematic and photographic. A subtle translucent violet digital-twin overlay (thin wireframe lines, glowing outlines, small data markers, hex 742A98 and B04BE0) sits on top of the real scene. Machines are sleek matte graphite with small violet accent lights. People wear plain work clothes and look natural and focused. Calm, precise, optimistic; no weapons, no text, no labels, no logos, no watermarks, no user interface screens with readable text.'
    ),
    "warehouse": (
        'A large modern warehouse seen from a high overhead three-quarter angle. Low flat mobile robots carry shelving units along aisles, each following a thin glowing violet route line on the floor. A worker in a high-visibility vest restocks a shelf in one aisle, and a second worker pushes a cart at the far end. Faint violet wireframe outlines mark the aisles as a digital twin. Clean, bright, orderly interior. Stylized high-quality 3D render in the look of a premium technology keynote, cinematic and photographic. A subtle translucent violet digital-twin overlay (thin wireframe lines, glowing outlines, small data markers, hex 742A98 and B04BE0) sits on top of the real scene. Machines are sleek matte graphite with small violet accent lights. People wear plain work clothes and look natural and focused. Calm, precise, optimistic; no weapons, no text, no labels, no logos, no watermarks, no user interface screens with readable text.'
    ),
}

LOOK = ' Keep the exact look of the opening image throughout: the same place, people, machines, lighting, color palette and the subtle translucent violet digital-twin overlay. Photoreal cinematic footage, natural human motion, rigid machines that keep their shape. No text, no captions, no logos, no music video effects.'

SHOTS = {
    "agriculture": (
        'Shot 1, 0 to 3 seconds: slow aerial dolly over long green crop rows at sunrise with a faint violet wireframe field twin; a four-wheeled field robot straddles one row. Cut to Shot 2, 3 to 6 seconds: low close-up beside the robot as it rolls along the row scanning the plants; one small patch lights up with a violet outline, the robot stops over it and sprays a fine mist onto only that patch. Cut to Shot 3, 6 to 8 seconds: medium shot of the farmer by the pickup truck at the field edge watching the robot, tablet in hand, in the golden morning light.'
        + LOOK
    ),
    "rescue": (
        'Shot 1, 0 to 3 seconds: slow aerial push over a collapsed building site at dusk; a drone sweeps a violet wireframe scan over the rubble. Cut to Shot 2, 3 to 6 seconds: low tracking shot behind a tracked crawler robot with headlights driving into the rubble as a glowing violet signal marker appears in the debris ahead of it and pulses. Cut to Shot 3, 6 to 8 seconds: three responders in helmets and orange jackets at a field table turn from their rugged tablet and one points toward the signal while another raises a radio.'
        + LOOK
    ),
    "perimeter": (
        'Shot 1, 0 to 3 seconds: slow aerial orbit at night around a secure facility ringed by sensor posts projecting violet detection fans; outside the fence an object sits inside a glowing violet ring. Cut to Shot 2, 3 to 6 seconds: low tracking shot following a four-legged robot trotting across the dark field toward the ring through drifting fog. Cut to Shot 3, 6 to 8 seconds: close-up of the robot stopping at the ring and scanning the object with a thin violet beam as the ring brightens.'
        + LOOK
    ),
    "quality": (
        'Shot 1, 0 to 3 seconds: slow dolly along a bright factory production line; metal parts glide on the conveyor under a row of inspection cameras whose violet light scans each part, a robot arm waits beside the line and a technician holds a tablet. Cut to Shot 2, 3 to 6 seconds: close-up at conveyor height; a glowing violet outline appears around one flawed part as it passes the cameras, and the robot arm smoothly reaches in, grips that part and lifts it off the line. Cut to Shot 3, 6 to 8 seconds: medium shot of the technician looking from the tablet to the part the robot arm holds up toward him, nodding.'
        + LOOK
    ),
    "inspection": (
        'Shot 1, 0 to 3 seconds: slow tracking shot at night following a four-legged inspection robot walking along the gravel path between transformers in an electrical substation, sensor mast scanning, light mist. Cut to Shot 2, 3 to 6 seconds: the robot stops and turns its sensor mast toward one transformer; a warm orange thermal overlay spreads across that transformer revealing a hot spot. Cut to Shot 3, 6 to 8 seconds: over-the-shoulder shot of an engineer in a hard hat by a service truck, looking up from a tablet toward the glowing transformer.'
        + LOOK
    ),
    "warehouse": (
        'Shot 1, 0 to 3 seconds: slow aerial dolly forward over the warehouse aisles; low flat mobile robots carrying shelving units glide along thin glowing violet route lines on the floor, a worker in a high-visibility vest restocks a shelf. Cut to Shot 2, 3 to 6 seconds: low tracking shot at floor level following one robot as its violet route bends smoothly around the worker and it drives past him into the next aisle, keeping a safe distance; the worker keeps working. Cut to Shot 3, 6 to 8 seconds: slow crane up and pull back revealing many robots flowing through the aisles in orderly lanes.'
        + LOOK
    ),
}


def api(path, payload=None):
    req = urllib.request.Request(
        "https://api.wavespeed.ai" + path,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
        method="POST" if payload is not None else "GET",
    )
    with urllib.request.urlopen(req, timeout=180) as r:
        return json.loads(r.read())


def wait(prediction, limit):
    deadline = time.time() + limit
    while time.time() < deadline:
        time.sleep(8)
        result = api(f"/api/v3/predictions/{prediction}/result")["data"]
        if result["status"] == "completed":
            return result["outputs"][0]
        if result["status"] == "failed":
            raise SystemExit(f"generation failed: {result.get('error')}")
    raise SystemExit("generation timed out")


def upload(path):
    """Upload a JPEG copy of the still; large PNG uploads fail intermittently."""
    jpg = tempfile.NamedTemporaryFile(suffix=".jpg", delete=False).name
    subprocess.run(["sips", "-s", "format", "jpeg", "-s", "formatOptions", "92", path, "--out", jpg],
                   check=True, capture_output=True)
    body = open(jpg, "rb").read()
    for attempt in range(5):
        try:
            boundary = f"----veoveo{int(time.time() * 1000)}"
            data = (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="still.jpg"\r\n'
                    "Content-Type: image/jpeg\r\n\r\n").encode() + body + f"\r\n--{boundary}--\r\n".encode()
            req = urllib.request.Request(
                "https://api.wavespeed.ai/api/v3/media/upload/binary", data=data, method="POST",
                headers={"Authorization": f"Bearer {KEY}",
                         "Content-Type": f"multipart/form-data; boundary={boundary}"})
            with urllib.request.urlopen(req, timeout=180) as r:
                return json.loads(r.read())["data"]["download_url"]
        except OSError as error:
            print(f"upload retry {attempt + 1}: {error}", flush=True)
            time.sleep(4 * (attempt + 1))
    raise SystemExit("upload failed")


def still(name):
    prediction = api(f"/api/v3/{IMAGE_MODEL}", {
        "prompt": STILLS[name], "aspect_ratio": "3:2", "resolution": "2k", "quality": "high",
        "output_format": "png"})["data"]["id"]
    urllib.request.urlretrieve(wait(prediction, 900), os.path.join(OUT, f"{name}.png"))
    return f"still {name}"


def video(name):
    prediction = api(f"/api/v3/{VIDEO_MODEL}", {
        "image": upload(os.path.join(OUT, f"{name}.png")), "prompt": SHOTS[name],
        "duration": 8, "resolution": "1080p", "generate_audio": False})["data"]["id"]
    urllib.request.urlretrieve(wait(prediction, 2400), os.path.join(OUT, f"{name}.mp4"))
    return f"video {name}"


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in ("still", "video"):
        raise SystemExit("usage: usecases.py still|video [name ...]")
    os.makedirs(OUT, exist_ok=True)
    names = sys.argv[2:] or list(STILLS)
    step = still if sys.argv[1] == "still" else video
    with ThreadPoolExecutor(len(names)) as pool:
        for line in pool.map(step, names):
            print(line, flush=True)


if __name__ == "__main__":
    main()
