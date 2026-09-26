#!/usr/bin/env python3
"""Generate the website hero art through WaveSpeed GPT Image 2.

The output lands at art/hero.png. Convert it to public/assets/hero.jpg (2048 px,
quality 84) and public/assets/hero-1200.jpg after reviewing it.
"""
import json, os, time, urllib.request

BASE = "https://api.wavespeed.ai"
MODEL = "openai/gpt-image-2/text-to-image"
KEY = os.environ["MEDIA_PROVIDER_API_KEY"]
OUT = os.path.dirname(os.path.abspath(__file__))

PROMPT = (
    "A single continuous city block seen from a low aerial three-quarter view. The left third is"
    " a translucent digital twin drawn in thin glowing violet wireframe lines, the right third is"
    " the same block as a real photographic street scene at dusk, and the middle blends smoothly"
    " from simulation into reality. A mixed team works together across the scene: a human"
    " operator holding a tablet, a four-legged robot, a small wheeled ground robot, a multirotor"
    " drone in flight, and a small uncrewed boat on a canal. Thin luminous violet data lines link"
    " every member to a floating translucent world model in the center: a stack of four glass"
    " planes with small glowing nodes."
    " Stylized high-quality 3D render in the look of a premium technology keynote slide:"
    " near-black slate background, hex 0D1117, with a soft deep-violet atmospheric glow."
    " Sleek matte graphite machines with small violet light accents, hex 742A98 and B04BE0."
    " Clean, calm, optimistic and forward-looking; nothing military, no weapons."
    " Wide panoramic composition: all important content sits inside the central horizontal band"
    " covering the middle 45 percent of the image height; the top and bottom are dark, empty"
    " background. No text, no labels, no logos, no watermarks, no user interface."
)


def api(path, payload=None):
    req = urllib.request.Request(
        BASE + path,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
        method="POST" if payload is not None else "GET",
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.loads(r.read())


def main():
    pid = api(f"/api/v3/{MODEL}", {
        "prompt": PROMPT, "aspect_ratio": "3:2", "resolution": "2k", "quality": "high",
        "output_format": "png", "enable_sync_mode": False, "enable_base64_output": False,
    })["data"]["id"]
    deadline = time.time() + 900
    while time.time() < deadline:
        time.sleep(8)
        result = api(f"/api/v3/predictions/{pid}/result")["data"]
        if result["status"] == "completed":
            urllib.request.urlretrieve(result["outputs"][0], f"{OUT}/hero.png")
            print(f"done -> {OUT}/hero.png")
            return
        if result["status"] == "failed":
            raise SystemExit(f"generation failed: {result.get('error')}")
    raise SystemExit("generation timed out")


if __name__ == "__main__":
    main()
