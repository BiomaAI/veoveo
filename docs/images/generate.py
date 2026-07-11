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
        "System architecture schematic with exactly one hexagon in the whole image. Left: three outlined boxes stacked vertically labeled SENSORS, SUMO, AGENTS, each with a thin amber arrow labeled PUSH into a tall outlined container labeled RECORDING HUB holding three stacked outlined stages labeled INGEST, PERSIST, SEGMENTS. Center-right: one outlined circle labeled GATEWAY. Above the circle, an outlined rounded rectangle labeled CONSOLE with a plain dark arrow down into the circle. Left of the circle, the single outlined hexagon, labeled AGENT, with a plain dark arrow into the circle labeled TOOLS. One plain dark unlabeled arrow leaves the circle rightward into a horizontal row of nine tiny outlined rectangles with the word MODULES written beneath the row. Bottom center: an outlined database cylinder labeled PLATFORM STORE, connected by thin plain dark lines up to the GATEWAY circle and to the MODULES row. One long amber arrow starts at the bottom edge of the RECORDING HUB container and ends at the MODULES row, labeled QUERIES. One amber dashed arrow starts at the PLATFORM STORE cylinder and ends at the AGENT hexagon, labeled WAKES, pointing at the hexagon. Amber appears only on the three PUSH arrows, the QUERIES arrow, and the WAKES arrow; every other line is dark slate."
    ),
    "world-model": (
        "1536*1024",
        "World model schematic, the centerpiece drawn large. Left: three outlined boxes stacked"
        " vertically labeled SENSORS, MEMORIES, DECISIONS, each with a thin arrow converging into"
        " the center. Center: one large dominant sphere drawn as a thin-line hexagonal wireframe"
        " lattice, labeled WORLD MODEL in larger letters beneath it — the only amber-outlined"
        " element. Right: one outlined hexagon labeled AGENT, receiving one amber arrow from the"
        " sphere labeled CONTEXT. From the AGENT hexagon one thin plain dark curved arrow returns"
        " to the DECISIONS box on the left — the agent's own choices become part of the model."
        " Exactly six labels: SENSORS, MEMORIES, DECISIONS, WORLD MODEL, CONTEXT, AGENT, each"
        " appearing exactly once. The amber accent appears only on the wireframe sphere and the"
        " CONTEXT arrow; everything else dark slate."
    ),
    "context-queried": (
        "1536*1024",
        "Schematic of context assembled by query. Right: a large outlined chamber labeled EPISODE"
        " containing an outlined hexagon labeled AGENT, with a small gauge attached to the"
        " chamber's edge labeled TOKEN BUDGET. Left: three outlined sources stacked vertically —"
        " a database cylinder labeled MEMORY with two tiny tags reading MISSIONS and BELIEFS, a"
        " stack of layered strata labeled RECORD with two tiny tags reading LATEST-AT and RANGE,"
        " and a horizontal filmstrip band labeled DECISION LOG. Between them: three thin dark"
        " arrows run left from the EPISODE chamber, one to each source, collectively labeled QUERY"
        " with one label; and from each source one amber arrow returns right into the chamber"
        " carrying two or three tiny row glyphs, collectively labeled ANSWERS with one label."
        " Each label appears exactly once. The amber accent appears only on the three returning"
        " answer arrows — tokens carry answers; everything else dark slate."
    ),
    "agent-loop": (
        "1536*1024",
        "Lifecycle schematic: five outlined circles placed at the five vertices of a regular pentagon, connected clockwise by curved dark slate arrows to form a single closed loop. Going clockwise from the top vertex the circles are labeled: WAKE, then ASSEMBLE, then EPISODE, then PERSIST, then SLEEP. There are exactly five circles and exactly five labels; every circle has one label; the words WAKE, ASSEMBLE, EPISODE, PERSIST, SLEEP each appear exactly once. In the pentagon's center, one outlined hexagon labeled AGENT. Below the pentagon, three small outlined database cylinders in a row labeled STATE, MEMORY, LOG, each connected by one thin plain line up to the AGENT hexagon only. Outside the loop at the upper left, three small outlined tags labeled TASK RESULT, TIMER, MESSAGE, each with a thin arrow pointing at the WAKE circle. Exactly one element in the whole image is amber: the outline of the EPISODE circle. Every arrow and every other outline is dark slate."
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
    "harness-poster": (
        "1536*1024",
        "The complete system as a single poster schematic, one grand loop. Far left: a region labeled WORLD — a small flat street grid with a few sensor dots and one camera glyph. Amber arrows labeled PUSH flow from WORLD into a stack of layered strata labeled RECORD. From RECORD one thin arrow passes through a small chamber labeled PERCEPTION and continues into the centerpiece: a large, perfectly circular thin-line wireframe sphere — a true circle, never stretched or oval — dominant at the center, with the text WORLD MODEL written in large dark capital letters directly beneath the sphere; this label is mandatory. One small outlined tag attached to the sphere's upper right is labeled MISSION, and above it an outlined rounded box labeled OPERATOR sends one short thin arrow labeled ASSIGN down to the MISSION tag. From the sphere one amber arrow labeled CONTEXT runs right into an outlined hexagon labeled AGENT. From AGENT one amber arrow enters a tall chamber labeled GATEWAY containing three small checkpoint stages stacked vertically and labeled AUTHENTICATE, POLICY, AUDIT, and exits into a region labeled CAPABILITIES: a compact grid of ten small outlined squares, each holding a tiny distinct instrument glyph — a camera, a compass rose, a database drum, a route with waypoints, a rising chart, a chain link, a filmstrip, an eye — followed by two dashed empty squares, room for capabilities not yet added. One long amber arrow labeled ACT begins at the top of the CAPABILITIES region, sweeps across the top of the poster from right to left, and ends with its arrowhead pointing down onto the WORLD region at the far left — closing the loop. Bottom center: one outlined database cylinder labeled EVIDENCE, connected by thin plain dark lines up to RECORD, to AGENT, and to GATEWAY. Each label appears exactly once. The amber accent appears only on the wireframe sphere and the loop arrows — PUSH, CONTEXT, and ACT; everything else dark slate. Crisp uniform line weight, sharp vector-like edges, high resolution."
    ),
    "exec-overview": (
        "1536*1024",
        "Holistic system loop schematic: five outlined nodes placed at the five vertices of a regular"
        " pentagon, connected clockwise by curved arrows into one closed loop. Clockwise from the top"
        " vertex: a small flat street-grid icon labeled WORLD, a stack of layered strata labeled"
        " RECORD, an outlined hexagon labeled AGENT, a small chamber holding three tiny checkpoint"
        " squares labeled GATEWAY, and a three by three mini grid of tiny squares labeled TOOLS; the"
        " loop closes from TOOLS back to WORLD. Exactly five nodes, each labeled once. In the"
        " pentagon's center, one outlined database cylinder labeled EVIDENCE, connected to every node"
        " by one thin plain dark line. The amber accent appears only on the five curved loop arrows —"
        " the autonomy loop itself; everything else dark slate."
    ),
    "capability-map": (
        "1536*1024",
        "Modular capability schematic in three parts. Center: one outlined circle labeled GATEWAY; directly beneath it, three stacked wide horizontal bars drawn as thin amber outlines with white interiors, labeled TASKS, ARTIFACTS, POLICY. Left of the gateway: one large dashed rounded boundary labeled EDGE containing a grid of exactly ten equal outlined rectangles arranged as two rows of five; reading left to right, top to bottom they are labeled row one MEDIA, PERCEPTION, GEODESY, SQL, PLANNING; row two FORECASTING, SHARING, RECORDINGS, CHARTS, VIEWER — ten rectangles, each word appearing exactly once. Right of the gateway: one large dashed rounded boundary labeled REMOTE containing three stacked rectangles: the top one outlined and labeled MCP SERVER, the middle one outlined and labeled PROVIDER, and the bottom one drawn with a dashed outline, empty except for one plus symbol — an open slot. One trunk line connects the EDGE boundary to the gateway circle and one trunk line connects the REMOTE boundary to the gateway circle. The amber accent appears only on the three contract bars and the plus symbol; everything else dark slate."
    ),
    "perception-flow": (
        "1536*1024",
        "Capability schematic for perception. Left: a stack of layered strata labeled RECORD."
        " Top center: one outlined hexagon labeled AGENT with a thin arrow down labeled ASK into a"
        " central outlined chamber labeled PERCEPTION, which contains exactly two small stages"
        " labeled DETECT and TRACK. One thin plain arrow runs from the RECORD stack right into the"
        " chamber. From the chamber, three amber arrows fan out to the right to three outlined"
        " boxes stacked vertically labeled DETECTIONS, ANNOTATIONS, CLIP; a small gray tag under"
        " DETECTIONS reads SQL and a small gray tag under ANNOTATIONS reads VIEWER. One thin plain"
        " curved arrow returns from the three output boxes back to the RECORD stack. Each label"
        " appears exactly once. The amber accent appears only on the three output arrows — the"
        " answers; everything else dark slate."
    ),
    "planning-flow": (
        "1536*1024",
        "Capability schematic for planning. Left: a small thin-line wireframe sphere labeled WORLD"
        " MODEL, and beneath it a small grid of table rows labeled OPTIONS with a gray tag reading"
        " SQL. Both send thin plain arrows right into a central outlined chamber labeled PLANNING,"
        " with one small outlined tag attached to its top edge labeled OBJECTIVE. One amber arrow"
        " leaves the chamber rightward into an outlined box labeled PLAN. From PLAN, one thin plain"
        " arrow runs up-right to an outlined hexagon labeled AGENT with the label EXECUTE on the"
        " arrow, and one thin plain arrow runs down-right to a small outlined database cylinder"
        " labeled MEMORY with the label WAYPOINTS on the arrow. Each label appears exactly once."
        " The amber accent appears only on the arrow from PLANNING to PLAN and the PLAN box outline"
        " — the decision; everything else dark slate."
    ),
    "gateway-gauntlet": (
        "1536*1024",
        "Defense-in-depth architecture schematic, read left to right as one continuous flow."
        " Far left: three client shapes stacked vertically — an outlined hexagon labeled AGENT, an"
        " outlined rounded rectangle labeled BROWSER, an outlined rectangle labeled CLIENT — each"
        " sending one thin arrow that converges on a single narrow opening in a tall vertical wall"
        " labeled INGRESS; the wall has exactly one opening. Center: one large outlined chamber"
        " labeled GATEWAY containing exactly three checkpoint stages in sequence connected by arrows,"
        " labeled in order AUTHENTICATE, POLICY, AUDIT; the flow enters the chamber on the left,"
        " passes through all three stages, and exits on the right. From the POLICY stage one short"
        " arrow deflects downward out of the flow, labeled REFUSED. Right: one large dashed boundary"
        " region labeled INTERNAL NETWORK containing a three by three grid of nine small unlabeled"
        " outlined rectangles; the single arrow from the gateway chamber into this region is labeled"
        " SIGNED IDENTITY. Below the gateway chamber, one outlined database cylinder labeled EVIDENCE"
        " receiving a thin arrow down from the AUDIT stage. The amber accent appears only on the"
        " three checkpoint stage outlines and the REFUSED deflection arrow — the enforcement path;"
        " every other line is dark slate."
    ),
    "capture-pipeline": (
        "1536*1024",
        "Horizontal pipeline schematic. Left: three outlined boxes stacked vertically labeled SENSORS, AGENTS, SUMO, each with a thin arrow labeled PUSH merging into the pipeline. The pipeline flows left to right through four outlined stages connected by arrows, labeled INGEST, PERSIST, LIVE SEGMENT, FROZEN SEGMENT. On the arrow between LIVE SEGMENT and FROZEN SEGMENT sits one small diamond checkpoint labeled VERIFY. Below the pipeline, one branch arrow drops from LIVE SEGMENT down to an outlined box labeled CATALOG, which sends one arrow right to an outlined box labeled QUERIES. The amber accent is used only for the LIVE SEGMENT and FROZEN SEGMENT stage outlines, the VERIFY diamond, and the arrows between them — the durable record and its proof; everything else dark slate."
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
    "deployment-map": (
        "1536*1024",
        "Deployment spectrum schematic: four flat 2D installation islands in a row, each a large rounded outlined base, labeled beneath in order EDGE, CLUSTER, AIR-GAP, HYBRID. Every island carries the same stack drawn identically: one small amber diamond mark at the center of the base, and outlined agent hexagons floating above the diamond — exactly one hexagon above the EDGE island, three hexagons above the CLUSTER island, two hexagons above the AIR-GAP island, and two hexagons above the HYBRID island. The EDGE base contains a single small outlined box; the CLUSTER base contains a row of three small outlined boxes; the AIR-GAP base is drawn with a double-line sealed border and connects to nothing; the HYBRID base has one dashed line rising to a small outlined box at the upper right labeled REMOTE. One continuous baseline under all four islands labeled ONE PLATFORM. The amber accent appears only on the four identical diamond marks — the same platform in every form; everything else dark slate."
    ),
    "operations-loop": (
        "1536*1024",
        "Dual-loop cognition schematic: two closed triangular loops side by side. Above the left loop a small heading tag reads REACTIVE; the loop has exactly three outlined circles connected clockwise by curved arrows, labeled DETECT, DECIDE, INTERVENE. Above the right loop a small heading tag reads PROACTIVE; the loop has exactly three outlined circles connected clockwise by curved arrows, labeled ANALYZE, PLAN, DISPATCH. Every word appears exactly once; every circle has one label. Between the two loops at the bottom, one small outlined tag reads NONSTOP. The amber accent appears only on the arrow from DECIDE to INTERVENE and the arrow from PLAN to DISPATCH — the two moments of action; everything else dark slate."
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
