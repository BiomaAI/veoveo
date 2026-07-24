#!/usr/bin/env python3
"""Generate dark-theme variants of the Autonomy Harness schematic figures.

Each `<name>.png` produced by `generate.py` gets a `<name>-dark.png` sibling
for GitHub's `<picture>` prefers-color-scheme switch. The recolor estimates
per-pixel ink coverage against the white canvas, maps neutral slate ink to
light slate, brightens the amber accent, and composites everything over the
GitHub dark canvas. Antialiasing survives; no re-generation is involved.

Usage:
    uv run --with numpy --with pillow --python 3.13 docs/images/darken.py [figure ...]

With no arguments every figure in FIGURES regenerates; naming figures
regenerates only those. Add a figure name to FIGURES when its light variant
gains a dark embed in the docs.
"""
import os
import sys

import numpy as np
from PIL import Image

OUT = os.path.dirname(os.path.abspath(__file__))

FIGURES = [
    "harness-poster",
    "operations-loop",
    "task-sleepwake",
    "agent-loop",
    "deployment-map",
]

DARK_CANVAS = np.array([13, 17, 23], np.float32)  # GitHub dark background
LIGHT_INK = np.array([201, 209, 217], np.float32)  # light slate line work
NEUTRAL_SAT = 34  # channel spread below this counts as neutral ink
ACCENT_GAIN = 1.35  # amber brightens for contrast on the dark canvas


def darken(name: str) -> None:
    src = os.path.join(OUT, f"{name}.png")
    dst = os.path.join(OUT, f"{name}-dark.png")
    im = np.asarray(Image.open(src).convert("RGB")).astype(np.float32)
    mn = im.min(axis=2)
    sat = im.max(axis=2) - mn
    alpha = np.clip((255.0 - mn) / 235.0, 0.0, 1.0)[..., None]
    neutral = (sat < NEUTRAL_SAT)[..., None]
    safe_a = np.maximum(alpha, 1e-3)
    accent = np.clip((im - 255.0 * (1 - safe_a)) / safe_a, 0, 255)
    accent = np.clip(accent * ACCENT_GAIN, 0, 255)
    ink = np.where(neutral, LIGHT_INK, accent)
    out = DARK_CANVAS * (1 - alpha) + ink * alpha
    Image.fromarray(out.astype(np.uint8)).save(dst)
    print(f"{name}-dark.png")


if __name__ == "__main__":
    names = sys.argv[1:] or FIGURES
    unknown = sorted(set(names) - set(FIGURES))
    if unknown:
        sys.exit(f"unknown figures: {', '.join(unknown)}")
    for figure in names:
        darken(figure)
