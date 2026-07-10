#!/usr/bin/env python3
"""Generate the Autonomy Harness schematic figures via wavespeed gpt-image-1.5.

The STYLE anchor and per-figure prompts below are the canonical Veoveo doc-image
style: engineering schematic, white background, uniform thin dark slate line
work, flat 2D, and exactly one amber accent placed only where it carries
meaning. Keep new figures in this voice.

Usage:
    MEDIA_PROVIDER_API_KEY=... python3 docs/images/generate.py [figure ...]

With no arguments every figure regenerates; naming figures regenerates only
those. Outputs land beside this script. Review every output for label accuracy
before shipping - the model likes duplicating node labels, and countable
geometries (a pentagon, a three-by-three grid) hold label counts far better
than rings.
"""
import json, os, sys, time, urllib.request

BASE = "https://api.wavespeed.ai"
MODEL = "openai/gpt-image-1.5/text-to-image"
KEY = os.environ["MEDIA_PROVIDER_API_KEY"]
OUT = os.path.dirname(os.path.abspath(__file__))

STYLE = (
    " Style: precise engineering schematic, like a figure in a technical standards document."
    " Clean white background. Uniform thin dark slate line work, hex 17212B, single stroke weight,"
    " flat 2D orthographic drawing, no perspective, no shading, no gradients, no glow, no 3D,"
    " no decorative icons, no fills except where specified. Exactly one accent color, amber hex B87514,"
    " used only where stated; everything else is dark slate line on white."
    " Small all-caps dark slate sans-serif labels, perfectly spelled, high legibility."
    " Only the exact labels specified appear; no other text, no lorem ipsum, no logos, no humans."
    " Austere, minimal, generous white space."
)

IMAGES = {
    "cover": (
        "1024*1536",
        "Vertical layered architecture schematic on a deep slate blue-black background, hex 0e141b,"
        " drawn in uniform thin pale gray lines with one amber accent, hex e2a14e. Four tiers connected"
        " by thin vertical arrows flowing downward. Top tier: four small outlined hexagons, labeled"
        " AGENTS. Second tier: one large outlined circle, labeled GATEWAY, with two small outlined tags"
        " beside it reading POLICY and AUDIT. Third tier: a horizontal row of nine small outlined"
        " rectangles, labeled TOOLS. Bottom tier: five stacked horizontal outlined layers, labeled"
        " RECORD. One thin amber arrow rises along the right side from the bottom tier back to the top"
        " tier, labeled WAKES — the amber line is the only colored element. Flat 2D, no glow, no fills,"
        " no icons. Small all-caps pale gray labels, perfectly spelled. Keep the lower quarter empty."
    ),
    "system-map": (
        "1536*1024",
        "System architecture schematic. Left: three outlined boxes stacked vertically labeled SENSORS, SUMO, AGENTS, each with a thin amber arrow labeled PUSH into a tall outlined container labeled RECORDING HUB holding three stacked outlined stages labeled INGEST, SPOOLER, SEGMENTS. Right of center: one outlined hexagon labeled AGENT with a plain dark arrow labeled TOOLS into an outlined circle labeled GATEWAY, and a plain dark unlabeled arrow from GATEWAY into a row of nine tiny outlined rectangles labeled SERVERS. Bottom center: an outlined database cylinder labeled SURREALDB connected by thin plain dark lines to GATEWAY and SERVERS. One separate long amber arrow curves from the RECORDING HUB container directly to the SERVERS row, bypassing the gateway, labeled QUERIES. Amber appears only on the three PUSH arrows, the hub stage outlines, and the QUERIES arrow; every other line is dark slate."
    ),
    "agent-loop": (
        "1536*1024",
        "Lifecycle schematic: five outlined circles placed at the five vertices of a regular pentagon, connected clockwise by curved arrows to form a single closed loop. Going clockwise from the top vertex the circles are labeled: WAKE, then ASSEMBLE, then EPISODE, then PERSIST, then SLEEP. There are exactly five circles and exactly five labels; every circle has one label; the words WAKE, ASSEMBLE, EPISODE, PERSIST, SLEEP each appear exactly once. In the pentagon's center, one outlined hexagon labeled AGENT. Outside the loop at the upper left, three small outlined tags labeled TASK RESULT, TIMER, MESSAGE, each with a thin arrow pointing at the WAKE circle. The amber accent appears only on the EPISODE circle outline and on the closing arrow from SLEEP to WAKE; everything else dark slate."
    ),
    "envelope": (
        "1536*1024",
        "Policy containment schematic: one large rounded rectangle boundary labeled POLICY ENVELOPE"
        " along its top edge. Inside, three outlined hexagons labeled AGENT with short dashed motion"
        " trails. On the right edge, one gap in the boundary labeled GATEWAY with a single thin arrow"
        " passing through to the outside, labeled AUDITED. Outside on the left, one arrow bouncing off"
        " the closed wall, labeled REFUSED. The amber accent is used only for the envelope boundary"
        " line itself; everything else dark slate."
    ),
    "servers-map": (
        "1536*1024",
        "Architecture schematic in two parts. Left: one large outlined circle labeled GATEWAY. Right: a three by three grid of nine equal outlined rectangles. One horizontal trunk line leaves the gateway circle and fans out into the grid. Reading the grid left to right, top to bottom, the nine rectangles are labeled: row one MEDIA, COORDINATES, DUCKDB; row two OPTIMIZATION, TIMESERIES, ARTIFACT; row three RECORDING, CHARTS, RERUN. Exactly nine rectangles, each with one label, each word appearing exactly once. The amber accent appears only on the gateway circle; everything else dark slate."
    ),
    "capture-pipeline": (
        "1536*1024",
        "Horizontal pipeline schematic. Left: three outlined boxes stacked vertically labeled SENSORS,"
        " AGENTS, SUMO, each with a thin arrow labeled PUSH merging into the pipeline. The pipeline"
        " flows left to right through four outlined stages connected by arrows, labeled PROXY, SPOOLER,"
        " LIVE SEGMENT, FROZEN SEGMENT. Below the pipeline, one branch arrow drops from LIVE SEGMENT"
        " down to an outlined box labeled CATALOG, which sends one arrow right to an outlined box"
        " labeled QUERIES. The amber accent is used only for the LIVE SEGMENT and FROZEN SEGMENT"
        " stage outlines and the arrow between them — the durable record; everything else dark slate."
    ),
    "task-sleepwake": (
        "1536*1024",
        "Sequence timeline schematic with two vertical lifelines: left lifeline topped by an outlined"
        " hexagon labeled AGENT, right lifeline topped by an outlined rectangle labeled SERVER."
        " Between them, top to bottom: a solid horizontal arrow left to right labeled CALL, a dashed"
        " return arrow right to left labeled TASK HANDLE, then the left lifeline becomes a dotted"
        " segment labeled SLEEP while the right lifeline shows a narrow activation bar labeled RUNNING,"
        " then a solid arrow right to left labeled WAKE, then a small outlined box on the left lifeline"
        " labeled RESULT. One thin downward arrow on the far left labeled TIME. The amber accent is"
        " used only for the WAKE arrow; everything else dark slate."
    ),
    "sumo-loop": (
        "1536*1024",
        "Closed control loop schematic: bottom third is a flat 2D street grid drawn in thin dark slate"
        " lines, one intersection marked with a small outlined square. Above it, exactly three outlined"
        " circles form a clockwise triangular loop connected by curved arrows, labeled PERCEIVE,"
        " DECIDE, ACT — each label exactly once, no duplicates. A thin arrow rises from the street grid"
        " into PERCEIVE labeled RECORD, and a thin arrow descends from ACT to the marked intersection"
        " labeled SIGNALS. One small outlined tag beside PERCEIVE reads CONGESTION. The amber accent is"
        " used only for the descending SIGNALS arrow and the marked intersection — the intervention;"
        " everything else dark slate."
    ),
    "deployment-forms": (
        "1536*1024",
        "Three deployment schematics side by side, drawn flat 2D. Left: one outlined box containing a"
        " small diamond mark, labeled COMPOSE below. Middle: a three by two grid of small outlined"
        " boxes with the same diamond mark in the center box, labeled HELM below. Right: one outlined"
        " box with a double-line border and no connecting lines, containing the same diamond mark,"
        " labeled OFFLINE below. A single baseline under all three labeled ONE PLATFORM. The amber"
        " accent is used only for the three identical diamond marks — the same platform in every form;"
        " everything else dark slate."
    ),
    "operations-loop": (
        "1536*1024",
        "Continuous operations cycle schematic: exactly three outlined circles connected clockwise by"
        " curved arrows into a loop, labeled DETECT, DECIDE, INTERVENE — each label exactly once, no"
        " duplicates. In the center of the loop, a small flat radar rose of concentric thin circles"
        " with a few signal dots. One small outlined tag under the loop reads NONSTOP. The amber accent"
        " is used only for the arrow from DECIDE to INTERVENE and two intercepted signal dots;"
        " everything else dark slate."
    ),
}


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
    only = set(sys.argv[1:])
    jobs = {}
    for name, (size, prompt) in IMAGES.items():
        if only and name not in only:
            continue
        resp = api(f"/api/v3/{MODEL}", {
            "prompt": prompt,
            "size": size,
            "quality": "high",
            "output_format": "png",
            "background": "opaque",
        })
        jobs[name] = resp["data"]["id"]
        print(f"submitted {name}: {jobs[name]}", flush=True)

    pending = dict(jobs)
    deadline = time.time() + 900
    while pending and time.time() < deadline:
        time.sleep(8)
        for name, pid in list(pending.items()):
            r = api(f"/api/v3/predictions/{pid}/result")
            st = r["data"]["status"]
            if st == "completed":
                dest = f"{OUT}/{name}.png"
                urllib.request.urlretrieve(r["data"]["outputs"][0], dest)
                print(f"done {name} -> {dest} ({os.path.getsize(dest)//1024} KB)", flush=True)
                del pending[name]
            elif st == "failed":
                print(f"FAILED {name}: {r['data'].get('error')}", flush=True)
                del pending[name]
    if pending:
        print("timed out:", pending)
        sys.exit(1)


if __name__ == "__main__":
    main()
