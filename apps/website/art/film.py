#!/usr/bin/env python3
"""Cut the film card's silent loop for the veoveo.ai hero from a render of the Veoveo film.

The card loops six shots of the film, each with its caption complete, joined by short
crossfades: the lens gathering the feeds, the harness framing the humans and robots, the
drone grid, the simulation wall, the digital twin and the end card. The end card's lens
cuts back to the opening lens, so the loop closes on a match. Clicking the card plays the
full film from YouTube.

    python3 art/film.py path/to/veoveo-film.mp4

It writes public/assets/cards/film.mp4 and its poster film.jpg with the use-case cards'
encoding: 960 px wide, H.264 CRF 26, no audio.
"""
import os, subprocess, sys

CARDS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "public", "assets", "cards")
# (start, end) seconds in the film.
SHOTS = [(1.5, 4.0), (16.8, 19.3), (50.8, 53.3), (69.6, 72.1), (99.8, 102.3), (106.8, 109.3)]
XFADE = 0.3


def main(film):
    inputs, chains = [], []
    for i, (start, end) in enumerate(SHOTS):
        inputs += ["-ss", str(start), "-t", str(end - start), "-i", film]
        chains.append(f"[{i}:v]settb=AVTB,setpts=PTS-STARTPTS,fps=30,scale=960:540:flags=lanczos[s{i}]")
    joined, elapsed = "s0", SHOTS[0][1] - SHOTS[0][0]
    for i, (start, end) in enumerate(SHOTS[1:], 1):
        chains.append(f"[{joined}][s{i}]xfade=transition=fade:duration={XFADE}:offset={elapsed - XFADE:.3f}[x{i}]")
        joined, elapsed = f"x{i}", elapsed + end - start - XFADE
    out = os.path.join(CARDS, "film.mp4")
    subprocess.run(["ffmpeg", "-v", "error", "-y", *inputs, "-filter_complex", ";".join(chains), "-map", f"[{joined}]",
                    "-an", "-c:v", "libx264", "-preset", "slow", "-crf", "26", "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart", out], check=True)
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", out, "-frames:v", "1", "-q:v", "4",
                    os.path.join(CARDS, "film.jpg")], check=True)
    print(f"{out}: {elapsed:.1f} s, {os.path.getsize(out) / 1e6:.2f} MB")


if __name__ == "__main__":
    main(sys.argv[1])
