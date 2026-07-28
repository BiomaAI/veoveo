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
        "High-level system topology in a strict left-to-right layout. Far left, one outlined camera"
        " labeled LIVE H264 sends one amber right-pointing arrow labeled DIRECT into an outlined"
        " chamber labeled STREAM. A second thin dashed arrow labeled OPTIONAL RECORDING drops from"
        " LIVE H264 into a small outlined box labeled FORWARDER, which sends a dark right-pointing"
        " arrow into a stack of layered strata labeled RECORD. STREAM sends one amber arrow into a"
        " small outlined box labeled RESULTS. RECORD sends exactly one dark arrow labeled SNAPSHOT"
        " into an outlined chamber labeled REASON. RESULTS sends exactly one separate dark arrow"
        " labeled GROUNDING directly into REASON. Both arrowheads visibly touch REASON. RESULTS"
        " does not connect to AGENT, and RECORD has no second arrow into REASON. Center-right,"
        " place exactly one hexagon labeled AGENT to the left of exactly one"
        " circle labeled GATEWAY. Join AGENT to GATEWAY with one dark right-pointing arrow labeled"
        " MCP. Put CONSOLE directly above GATEWAY and PLATFORM STORE directly below it, each joined"
        " by one plain vertical line. At far right, draw one large dashed rounded container labeled"
        " HOSTED SERVERS containing one unified abstract layered capability-plane glyph. Join"
        " GATEWAY to HOSTED SERVERS with one dark right-pointing arrow labeled FULL MCP. Join"
        " REASON to AGENT with one plain dark arrow labeled EVIDENCE."
        " HARD INVARIANT: STREAM has no dependency arrow from RECORD, and the DIRECT path never"
        " passes through RECORD. Every specified label appears exactly once. No extra nodes or text."
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
        " labeled LIVE flow from WORLD into a small chamber labeled STREAM, then continue into the"
        " centerpiece: a large, perfectly circular thin-line wireframe sphere — a true circle, never"
        " stretched or oval — dominant at the center, with the mandatory text WORLD MODEL in large"
        " dark capitals directly beneath it. A separate thin dashed arrow labeled OPTIONAL drops"
        " from WORLD into a stack of layered strata labeled RECORD. From STREAM one plain dark arrow"
        " labeled RESULTS and from RECORD one plain dark arrow labeled SNAPSHOT converge on a small"
        " outlined chamber labeled REASON beside the sphere; one plain arrow from REASON enters the"
        " sphere. One small outlined tag attached to the sphere's upper"
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
        " cylinder labeled EVIDENCE, connected by thin plain dark lines up to RECORD, REASON, AGENT, and"
        " GATEWAY. Each label appears exactly once. The amber accent appears only on the wireframe"
        " sphere and the loop arrows LIVE, CONTEXT, ACT; everything else dark slate. Crisp uniform line"
        " weight, sharp vector-like edges, high resolution."
    ),
    "capability-map": (
        "1536*1024",
        "Hosted capability schematic in three parts. Center: one outlined circle labeled GATEWAY;"
        " directly beneath it, four stacked wide horizontal bars drawn as thin amber outlines with"
        " white interiors, labeled from top to bottom FULL MCP, DOMAIN ADMIN, DURABLE TASKS, ARTIFACTS"
        " + POLICY. Left of the gateway: one large dashed rounded boundary labeled HOSTED containing"
        " a rigid grid of exactly seventeen equal outlined rectangles. The first five rows contain"
        " three aligned boxes each; a sixth centered row contains exactly two aligned boxes."
        " HARD LAYOUT INVARIANT: the HOSTED boundary must be large enough for all six rows. Reading"
        " left to right, top to bottom: row one MEDIA, STREAM, REASON; row two TIMESERIES, DUCKDB,"
        " OPTIMIZATION; row three FRAMES, MAP, DATASHEET; row four ARTIFACT, RECORDING, CHARTS;"
        " row five RERUN, TIME, VIEW; row six UAV SIM, SIMULATION VIEW. Draw all six rows inside"
        " the boundary. Every one of those seventeen labels is mandatory. Do not omit or duplicate"
        " any label, create a seventh row, or draw any extra capability"
        " box. Right of the"
        " gateway: one large dashed"
        " rounded boundary labeled REMOTE containing three stacked rectangles: the top one outlined"
        " and labeled MCP SERVER, the middle one outlined and labeled PROVIDER, and the bottom one"
        " drawn with a dashed outline, empty except for one plus symbol — an open slot. One trunk line"
        " connects the HOSTED boundary to the GATEWAY circle and one trunk line connects the REMOTE"
        " boundary to the GATEWAY circle. Every specified label appears exactly once. The amber accent"
        " appears only on the four contract bars and the plus symbol; everything else is dark slate."
    ),
    "stream-flow": (
        "1536*1024",
        "Live and replay stream-processing schematic. Left: one outlined camera labeled LIVE"
        " sends a right-pointing arrow labeled H264 into a large central chamber labeled STREAM."
        " Directly beneath STREAM, centered and fully inside the canvas, place one stack of layered"
        " strata labeled RECORD. Draw one short dark arrow labeled REPLAY from RECORD upward into"
        " STREAM. Draw a second, parallel thin dark dashed arrow labeled OPTIONAL from STREAM"
        " downward into RECORD, with its arrowhead visibly touching the RECORD stack. These two"
        " vertical arrows point in opposite directions and must both end on their destination."
        " Inside STREAM, place exactly"
        " three small stages left to right labeled GRAPH, DETECT, TRACK. From STREAM, one amber"
        " arrow goes right into a wide outlined screen labeled APP; inside that screen draw one"
        " plain video frame rectangle with two small outline boxes over it and the single label"
        " OVERLAYS. A second amber arrow goes down-right into an outlined box labeled RESULTS."
        " No arrow leaves the canvas and no arrow runs between RECORD and LIVE. Every label"
        " appears exactly once. The amber accent appears only on the APP and RESULTS arrows;"
        " everything else dark slate."
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
        "Horizontal durable-recording pipeline. Left: exactly three outlined producer boxes stacked"
        " vertically labeled SENSORS, AGENTS, SIMULATION. Their arrows converge on one outlined"
        " chamber labeled LOCAL FORWARDER with one small tag labeled BOUNDED QUEUE. One arrow labeled"
        " OAUTH + PROTOBUF goes right into an outlined chamber labeled RECORDING HUB. Inside the Hub"
        " place exactly three stages left to right labeled JOURNAL, MATERIALIZE, CHECKPOINT. A dark"
        " arrow labeled ACK returns from CHECKPOINT to LOCAL FORWARDER. From CHECKPOINT one arrow"
        " continues right into a stack of exactly three small layers labeled IMMUTABLE PARTS."
        " Then one amber arrow labeled 1 HOUR OR 192 MIB enters a small diamond labeled IDR, followed"
        " by one amber arrow into a stack of exactly three larger layers labeled ARCHIVE SHARDS."
        " Below ARCHIVE SHARDS place one outlined box labeled CATALOG joined by one plain dark line."
        " HARD INVARIANT: ACK happens only after JOURNAL, MATERIALIZE, and CHECKPOINT. No live segment,"
        " frozen segment, ingest socket, persist-first, or verify label appears. The amber accent is"
        " used only for the IDR boundary and archive-shard path; everything else dark slate."
    ),
    "grounding-flow": (
        "1536*1024",
        "Grounded video reasoning schematic in a strict left-to-right layout. Far left top: one"
        " outlined camera labeled LIVE H264 sends one amber arrow labeled DIRECT into a chamber"
        " labeled STREAM. Far left bottom: one stack of layered strata labeled RECORD sends one"
        " dark arrow labeled REPLAY upward into STREAM. STREAM sends one arrow right into an"
        " outlined box labeled TYPED RESULTS, containing three tiny unlabelled row glyphs. RECORD"
        " also sends one dark arrow labeled SNAPSHOT directly into a large chamber labeled REASON"
        " on the right. TYPED RESULTS sends one dark arrow labeled OPTIONAL GROUNDING into REASON."
        " Inside REASON place exactly three small stages labeled SAMPLE, WORLD MODEL, VALIDATE."
        " From REASON one amber arrow exits right into a stack of three outlined result cards"
        " collectively labeled GROUNDED OUTPUT. Beneath GROUNDED OUTPUT place exactly three tiny"
        " tags labeled EVENTS, ANSWERS, RRD. HARD INVARIANT: live media reaches STREAM directly and"
        " never passes through RECORD; REASON always receives SNAPSHOT while grounding is optional."
        " Every specified label appears exactly once. No extra nodes, arrows, or text."
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
    "integration-matrix": (
        "1536*1024",
        "Two-panel comparison schematic, left and right panels of equal size separated by generous"
        " white space and one thin vertical divider line."
        " HARD LAYOUT INVARIANT: each panel contains a top row of exactly four small outlined"
        " squares and a bottom row of exactly six small outlined squares, never five, never seven;"
        " the two panels have identical square counts and identical row spacing."
        " Left panel: one small heading tag at top reads POINT-TO-POINT. Every one of the four top"
        " squares is joined to every one of the six bottom squares by one thin dark straight line,"
        " twenty-four lines in total, forming a dense crossing mesh; every line begins visibly"
        " touching the bottom edge of a top square and ends visibly touching the top edge of a"
        " bottom square. Beneath the left panel one small outlined tag reads N × M INTEGRATIONS."
        " Right panel: one small heading tag at top reads ONE PROTOCOL. Between the two rows spans"
        " one wide horizontal bar drawn as a thin amber outline with white interior, labeled"
        " MCP + HARNESS. Each top square sends exactly one thin dark vertical line down whose ends"
        " visibly touch the square and the top edge of the bar; each bottom square sends exactly"
        " one thin dark vertical line up whose ends visibly touch the square and the bottom edge of"
        " the bar; no line crosses another and no line stops short of the bar."
        " Beneath the right panel one small outlined tag reads N + M CONTRACTS."
        " Each specified label appears exactly once, no duplicates. The amber accent appears only"
        " on the MCP + HARNESS bar outline in the right panel; every other line is dark slate."
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
