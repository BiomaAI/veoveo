#!/usr/bin/env python3
"""Generate the Autonomy Harness schematic figures via WaveSpeed GPT Image 2.

The STYLE anchor and per-figure prompts below are the canonical Veoveo doc-image
style: engineering schematic, white background, uniform thin dark slate line
work, flat 2D, and exactly one amber accent placed only where it carries
meaning. Keep new figures in this voice.

Usage:
    uv run --env-file .env --python 3.13 docs/images/generate.py [figure ...]

With no arguments every figure regenerates; naming figures regenerates only
those. Outputs land beside this script. Review every output for label accuracy
before shipping - the model likes duplicating node labels, and countable
geometries (a pentagon, a three-by-three grid) hold label counts far better
than rings.
"""
import json, os, sys, time, urllib.request

BASE = "https://api.wavespeed.ai"
MODEL = "openai/gpt-image-2/text-to-image"
KEY = os.environ["MEDIA_PROVIDER_API_KEY"]
OUT = os.path.dirname(os.path.abspath(__file__))

ASPECT_RATIOS = {
    "1024*1536": "2:3",
    "1536*1024": "3:2",
}

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
        " drawn in uniform thin pale gray lines with one amber accent, hex e2a14e. The dark background"
        " is mandatory and overrides any general white-background style instruction. Four tiers are"
        " connected by thin vertical arrows flowing downward. Top tier: exactly four small outlined"
        " hexagons in one row, never five, with the single group label AGENTS. Second tier: exactly one"
        " large outlined circle, labeled GATEWAY, with exactly two small outlined tags beside it reading"
        " POLICY and AUDIT. Third tier: exactly nine small outlined rectangles in one horizontal row,"
        " never ten, with the single group label TOOLS. No other hexagons or rectangles appear in those"
        " two rows. Bottom tier: five stacked horizontal outlined layers, labeled RECORD. One thin amber"
        " arrow rises along the right side from the bottom tier back to the top tier, labeled WAKES —"
        " the amber line is the only colored element. Flat 2D, no glow, no fills, no icons. Small"
        " all-caps pale gray labels, perfectly spelled. Keep the lower quarter empty."
    ),
    "system-map": (
        "1536*1024",
        "High-level system topology in a strict left-to-right layout. Far left, stack three outlined"
        " producer boxes labeled SENSORS, SUMO, AGENTS. Each sends one amber right-pointing arrow"
        " labeled PUSH into one tall RECORDING HUB container beside them. The hub contains three"
        " stacked stages labeled INGEST, PERSIST, SEGMENTS. Center, place exactly one hexagon labeled"
        " AGENT to the left of exactly one circle labeled GATEWAY. Join AGENT to GATEWAY with one"
        " dark right-pointing arrow labeled MCP. Put CONSOLE directly above GATEWAY and join it to"
        " GATEWAY with one plain dark vertical line labeled SESSION, without arrowheads. Put one"
        " database cylinder labeled"
        " PLATFORM STORE below GATEWAY, with DURABLE STATE + AUDIT beneath it, joined to GATEWAY by"
        " one plain dark vertical line. At right, draw one large dashed rounded container labeled"
        " 14 HOSTED SERVERS. Inside it, draw one unified abstract capability-plane glyph made from"
        " several thin unlabelled horizontal layers; do not draw separate countable server boxes."
        " Join GATEWAY to 14 HOSTED SERVERS with one dark right-pointing arrow labeled MCP + ADMIN."
        " This overview intentionally omits recording-query and task-wake cross-links; do not render"
        " QUERIES or WAKES. No direct line joins RECORDING HUB to AGENT or GATEWAY. No extra nodes,"
        " arrows, arrowheads, boxes, or labels."
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
        "Lifecycle schematic: exactly five outlined circles placed at the five vertices of a regular"
        " pentagon, connected clockwise by curved dark slate arrows to form one clean closed loop."
        " Going clockwise from the top vertex the circles are labeled WAKE, ASSEMBLE, EPISODE, PERSIST,"
        " SLEEP. There are exactly five circles and exactly five lifecycle labels; every circle has one"
        " label and each word appears exactly once. Inside the pentagon, place one outlined hexagon"
        " labeled AGENT above exactly three small outlined database cylinders in a row labeled STATE,"
        " MEMORY, LOG. Join each cylinder to AGENT with one short thin plain line. Every storage line"
        " begins visibly touching the bottom edge of AGENT and ends visibly touching the top rim of its"
        " own cylinder; no gap appears at either endpoint. Those three storage lines remain entirely"
        " inside the loop and must never touch or cross a lifecycle arrow, circle, or outer edge."
        " Outside the loop at upper left, place three small outlined tags labeled TASK"
        " RESULT, TIMER, MESSAGE, each with a thin arrow pointing only to the WAKE circle. Exactly one"
        " element in the whole image is amber: the outline of the EPISODE circle. Every arrow and every"
        " other outline is dark slate."
    ),
    "harness-poster": (
        "1536*1024",
        "The complete system as a single poster schematic, one grand loop. Far left: a region labeled"
        " WORLD — a small flat street grid with a few sensor dots and one camera glyph. Amber arrows"
        " labeled PUSH flow from WORLD into a stack of layered strata labeled RECORD. From RECORD one"
        " thin arrow passes through a small chamber labeled PERCEPTION and continues into the"
        " centerpiece: a large, perfectly circular thin-line wireframe sphere — a true circle, never"
        " stretched or oval — dominant at the center, with the mandatory text WORLD MODEL in large"
        " dark capitals directly beneath it. One small outlined tag attached to the sphere's upper"
        " right is labeled MISSION. Above it, an outlined rounded box labeled OPERATOR sends one short"
        " thin arrow labeled ASSIGN down to the MISSION tag. From the sphere one amber arrow labeled"
        " CONTEXT runs right into an outlined hexagon labeled AGENT. From AGENT one amber arrow enters"
        " a tall chamber labeled GATEWAY containing three small checkpoint stages stacked vertically"
        " and labeled AUTHENTICATE, POLICY, AUDIT. It exits into a region labeled CAPABILITIES. That"
        " region contains exactly eight solid outlined glyph cells arranged four by two — camera,"
        " compass rose, database drum, route with waypoints, rising chart, chain link, filmstrip, eye —"
        " followed below by exactly two dashed empty cells. One continuous long amber arrow labeled ACT"
        " has its tail visibly touching the top edge of CAPABILITIES, rises vertically, bends left, and"
        " sweeps across the top of the poster from right to left. The CAPABILITIES end is a plain tail"
        " with no arrowhead. The path ends above WORLD with its only arrowhead visibly touching and"
        " pointing down onto WORLD. HARD INVARIANT: the entire ACT path has exactly one arrowhead, at"
        " WORLD, and no arrowhead at CAPABILITIES. The ACT path has no dangling segment and no second"
        " arrow. Bottom center: one outlined database"
        " cylinder labeled EVIDENCE, connected by thin plain dark lines up to RECORD, AGENT, and"
        " GATEWAY. Each label appears exactly once. The amber accent appears only on the wireframe"
        " sphere and the loop arrows PUSH, CONTEXT, ACT; everything else dark slate. Crisp uniform line"
        " weight, sharp vector-like edges, high resolution."
    ),
    "capability-map": (
        "1536*1024",
        "Hosted capability schematic in three parts. Center: one outlined circle labeled GATEWAY;"
        " directly beneath it, four stacked wide horizontal bars drawn as thin amber outlines with"
        " white interiors, labeled from top to bottom FULL MCP, DOMAIN ADMIN, DURABLE TASKS, ARTIFACTS"
        " + POLICY. Left of the gateway: one large dashed rounded boundary labeled HOSTED containing"
        " a rigid grid of exactly fourteen equal outlined rectangles. The first four rows contain"
        " three aligned boxes each; a fifth centered row contains exactly two aligned boxes."
        " HARD LAYOUT INVARIANT: the HOSTED boundary must be large enough for all five rows, and no"
        " label may wrap onto a sixth row. Reading"
        " left to right, top to bottom: row one MEDIA, PERCEPTION, TIMESERIES; row two DUCKDB,"
        " OPTIMIZATION, FRAMES; row three MAP, DATASHEET, ARTIFACT; row four RECORDING, CHARTS, RERUN;"
        " row five TIME, VIEW. Draw all five rows inside the boundary. The fourth row must show"
        " RECORDING, CHARTS, and RERUN side by side; the fifth row must show TIME and VIEW side by"
        " side. DUCKDB, DATASHEET, TIME, and VIEW are mandatory. Do not omit any label, create a"
        " sixth row, or draw any extra capability"
        " box. Right of the"
        " gateway: one large dashed"
        " rounded boundary labeled REMOTE containing three stacked rectangles: the top one outlined"
        " and labeled MCP SERVER, the middle one outlined and labeled PROVIDER, and the bottom one"
        " drawn with a dashed outline, empty except for one plus symbol — an open slot. One trunk line"
        " connects the HOSTED boundary to the GATEWAY circle and one trunk line connects the REMOTE"
        " boundary to the GATEWAY circle. Every specified label appears exactly once. The amber accent"
        " appears only on the four contract bars and the plus symbol; everything else is dark slate."
    ),
    "perception-flow": (
        "1536*1024",
        "Capability schematic for perception. Left: a stack of layered strata labeled RECORD."
        " Top center: one outlined hexagon labeled AGENT with a thin arrow down labeled ASK into a"
        " central outlined chamber labeled PERCEPTION, which contains exactly two small stages"
        " labeled DETECT and TRACK. One thin plain arrow runs from the RECORD stack right into the"
        " chamber. From the chamber, three amber arrows fan out to the right to three outlined"
        " boxes stacked vertically labeled DETECTIONS, ANNOTATIONS, CLIP; a small gray tag under"
        " DETECTIONS reads SQL and a small gray tag under ANNOTATIONS reads VIEWER. No arrow connects"
        " one output box to another. A shared dark output bracket on the right collects the three"
        " boxes. From that bracket, one thin plain curved return arrow routes around the bottom of the"
        " figure and ends with its arrowhead visibly touching the RECORD stack. It must not connect"
        " DETECTIONS to ANNOTATIONS or CLIP. Each label appears exactly once. The amber accent appears"
        " only on the three outward answer arrows; everything else dark slate."
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
        " amber arrow deflects downward, ends at the label REFUSED, and stops. It never touches"
        " EVIDENCE. Right: one large dashed boundary"
        " region labeled INTERNAL NETWORK containing a three by three grid of nine small unlabeled"
        " outlined rectangles; the single arrow from the gateway chamber into this region is labeled"
        " SIGNED IDENTITY. Below the gateway chamber, one outlined database cylinder labeled EVIDENCE"
        " receives a separate dark vertical arrow whose source visibly touches AUDIT and whose"
        " arrowhead visibly touches EVIDENCE. The POLICY-to-REFUSED and AUDIT-to-EVIDENCE paths remain"
        " distinct and never join. The amber accent appears only on the"
        " three checkpoint stage outlines and the REFUSED deflection arrow — the enforcement path;"
        " every other line is dark slate."
    ),
    "capture-pipeline": (
        "1536*1024",
        "Horizontal pipeline schematic. Left: exactly three outlined boxes stacked vertically and"
        " labeled SENSORS, AGENTS, SUMO. Each has one thin right-pointing arrow carrying its own visible"
        " PUSH label, so the word PUSH appears exactly three times. All three arrows converge visibly"
        " on one dark merge junction, and one trunk arrow leaves that junction with"
        " its arrowhead visibly touching INGEST. No producer arrow is detached and every producer"
        " arrowhead points toward INGEST. The pipeline continues left to right through four outlined"
        " stages labeled INGEST, PERSIST, LIVE SEGMENT, FROZEN SEGMENT. On the arrow between LIVE"
        " SEGMENT and FROZEN SEGMENT sits one small diamond checkpoint labeled VERIFY. Below the"
        " pipeline, one branch arrow drops from LIVE SEGMENT into an outlined box labeled CATALOG,"
        " which sends one arrow right into an outlined box labeled QUERIES. The amber accent is used"
        " only for the LIVE SEGMENT and FROZEN SEGMENT stage outlines, the VERIFY diamond, and the"
        " arrows between them — the durable record and its proof; everything else dark slate."
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
            "prompt": prompt + STYLE,
            "aspect_ratio": ASPECT_RATIOS[size],
            "resolution": "2k",
            "quality": "high",
            "output_format": "png",
            "enable_sync_mode": False,
            "enable_base64_output": False,
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
