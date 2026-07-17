#!/usr/bin/env python3
"""Render the isolated Veoveo UAF/SysML reference architecture bundle.

The renderer uses only the Python standard library. Catalog CSV files remain
human-reviewable inputs; SVG, HTML, and XMI are deterministic derived artifacts.
"""

from __future__ import annotations

import csv
import html
import textwrap
import xml.etree.ElementTree as ET
from collections.abc import Iterable, Sequence
from pathlib import Path

ARCH = Path(__file__).resolve().parents[1]
CATALOGS = ARCH / "catalogs"
DIAGRAMS = ARCH / "diagrams"
MODEL = ARCH / "model"
VERSION = "0.1.0"
REVISION_DATE = "2026-07-17"
SOURCE_COMMIT = "f19c1b2d68c1bf4f6ad133712746c5e377bb4f3a"


def read_csv(name: str) -> list[dict[str, str]]:
    with (CATALOGS / name).open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


COMPONENTS = read_csv("software-components.csv")
INTERFACES = read_csv("interfaces-and-protocols.csv")
REQUIREMENTS = read_csv("requirements-traceability.csv")
GLOSSARY = read_csv("model-glossary.csv")

CAPABILITIES = [
    (
        "VV-CAP-001",
        "Governed MCP access",
        "Policy-scoped discovery and use of complete MCP domain surfaces.",
    ),
    (
        "VV-CAP-002",
        "Durable asynchronous work",
        "Long operations survive restart with explicit recovery semantics.",
    ),
    (
        "VV-CAP-003",
        "Governed artifacts",
        "Outputs retain identity, integrity, access, release, retention, and sharing policy.",
    ),
    (
        "VV-CAP-004",
        "Governed world recording",
        "Sensor and simulation evidence is pushed into durable RRD recordings.",
    ),
    (
        "VV-CAP-005",
        "Bounded analytics and decisions",
        "Arbitrary SQL, forecasting, planning, and charts execute inside owned limits.",
    ),
    (
        "VV-CAP-006",
        "Local perception",
        "Recorded video is decoded, detected, tracked, and published inside the installation.",
    ),
    (
        "VV-CAP-007",
        "Autonomous agent operation",
        "Bounded episodes persist scheduling, wakes, work, memory, and evidence.",
    ),
    (
        "VV-CAP-008",
        "Secure enterprise integration",
        "Installation-owned identity, policy, audit, tenancy, and internal trust compose.",
    ),
    (
        "VV-CAP-009",
        "Operations oversight",
        "Operators inspect and administer live work without exposing bearer credentials.",
    ),
    (
        "VV-CAP-010",
        "Connected and offline deployment",
        "One Helm service graph runs in local k3d or fielded Kubernetes with verified offline transfer.",
    ),
    (
        "VV-CAP-011",
        "Geospatial logistics",
        "Governed data releases support geodesy, restrictions, routing, and logistics matrices.",
    ),
    (
        "VV-CAP-012",
        "Extensible hosted domains",
        "Shared Rust contracts and validated Python models add servers without weakening canonical protocol behavior.",
    ),
]

ACTIVITIES = [
    (
        "VV-OPA-001",
        "Authenticate and authorize request",
        "Resolve identity, tenant, profile, scopes, policy, and audit context.",
    ),
    (
        "VV-OPA-002",
        "Discover and invoke domain capability",
        "Use tools, resources, prompts, completions, tasks, or subscriptions.",
    ),
    (
        "VV-OPA-003",
        "Execute and recover durable work",
        "Persist task ownership, transitions, recovery class, result, and event order.",
    ),
    (
        "VV-OPA-004",
        "Publish and share governed output",
        "Create artifact occurrence, store bytes, apply access, and release deliberately.",
    ),
    (
        "VV-OPA-005",
        "Ingest and query world evidence",
        "Push Rerun observations, freeze segments, authorize, and query logical recordings.",
    ),
    (
        "VV-OPA-006",
        "Analyze, route, and plan",
        "Transform governed inputs into analytical, logistical, or planning products.",
    ),
    (
        "VV-OPA-007",
        "Detect and track recorded objects",
        "Extract authorized video, execute local inference, and publish annotations.",
    ),
    (
        "VV-OPA-008",
        "Run bounded agent episode",
        "Wake, assemble context, reason, invoke work, detach, persist, and record evidence.",
    ),
    (
        "VV-OPA-009",
        "Administer installation",
        "Inspect health, work, topology, policy, audit, and authorized domain configuration.",
    ),
    (
        "VV-OPA-010",
        "Deploy and verify installation",
        "Bootstrap, configure, validate, observe, and transfer connected or offline forms.",
    ),
]

SERVICES = [
    (
        "VV-SVC-001",
        "MCP aggregation service",
        "Discovers hosted surfaces and projects policy-scoped profiles.",
    ),
    (
        "VV-SVC-002",
        "Identity and authorization service",
        "Federates OIDC and issues resource-bound external and internal tokens.",
    ),
    (
        "VV-SVC-003",
        "Durable work coordination service",
        "Coordinates task state, recovery, input, cancellation, result, and notifications.",
    ),
    (
        "VV-SVC-004",
        "Artifact governance service",
        "Controls artifact identity, bytes, grants, release, capabilities, and sharing.",
    ),
    (
        "VV-SVC-005",
        "Recording durability service",
        "Authenticates world streams and maintains crash-decodable governed recordings.",
    ),
    (
        "VV-SVC-006",
        "Domain compute service",
        "Runs analytical, forecasting, optimization, frame, and chart workloads.",
    ),
    (
        "VV-SVC-007",
        "Agent execution service",
        "Runs restart-safe bounded episodes over governed context and tools.",
    ),
    (
        "VV-SVC-008",
        "Operations console service",
        "Projects safe operational state and explicit administrative actions.",
    ),
    (
        "VV-SVC-009",
        "Deployment and assurance service",
        "Builds, deploys, transfers, validates, and verifies installation forms.",
    ),
    (
        "VV-SVC-010",
        "Telemetry and audit service",
        "Captures decision evidence and approved operational signals.",
    ),
    (
        "VV-SVC-011",
        "Geospatial logistics service",
        "Governs map releases and serves routing and spatial operations.",
    ),
    (
        "VV-SVC-012",
        "Recorded perception service",
        "Runs local bounded detection and tracking over governed recordings.",
    ),
]

ACTUAL_RESOURCES = [
    ("VV-AR-001", "k3d local development installation", "VV-CMP-035"),
    ("VV-AR-002", "Kubernetes connected installation", "VV-CMP-036"),
    ("VV-AR-003", "Kubernetes offline installation", "VV-CMP-036"),
]

ROLES = [
    ("VV-EXT-MCP-CLIENT", "MCP client"),
    ("VV-EXT-BROWSER", "Operator browser"),
    ("VV-EXT-OAUTH-CLIENT", "OAuth client"),
    ("VV-EXT-AUTHORIZED-USER", "Authorized artifact user"),
    ("VV-EXT-LINK-HOLDER", "Public link holder"),
    ("VV-EXT-PRODUCER", "Sensor or simulation producer"),
    ("VV-CMP-HOSTED", "Hosted MCP server role"),
    ("VV-CMP-PLATFORM", "Platform service role"),
]


class Svg:
    def __init__(self, width: int, height: int, title: str, desc: str, code: str):
        self.width = width
        self.height = height
        self.title = title
        self.desc = desc
        self.code = code
        self.parts: list[str] = []
        self.parts.append(
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
            f'width="{width}" height="{height}" role="img" aria-labelledby="title desc">'
        )
        self.parts.append(f'<title id="title">{html.escape(title)}</title>')
        self.parts.append(f'<desc id="desc">{html.escape(desc)}</desc>')
        self.parts.append(
            """<defs>
<marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#21465b"/></marker>
<marker id="arrow-muted" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#687983"/></marker>
<filter id="shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="#0b2534" flood-opacity="0.12"/></filter>
</defs>
<style>
.title{font:700 30px Arial,Helvetica,sans-serif;fill:#102f40}.subtitle{font:400 15px Arial,Helvetica,sans-serif;fill:#4d626d}.label{font:700 14px Arial,Helvetica,sans-serif;fill:#102f40;letter-spacing:.04em}.node-title{font:700 17px Arial,Helvetica,sans-serif;fill:#102f40}.node-id{font:700 12px Arial,Helvetica,sans-serif;fill:#176b68;letter-spacing:.04em}.body{font:400 14px Arial,Helvetica,sans-serif;fill:#2d4653}.small{font:400 12px Arial,Helvetica,sans-serif;fill:#526a76}.tiny{font:400 10px Arial,Helvetica,sans-serif;fill:#526a76}.inverse{fill:#ffffff}.edge{stroke:#21465b;stroke-width:2.2;fill:none;marker-end:url(#arrow)}.edge-muted{stroke:#687983;stroke-width:1.6;fill:none;marker-end:url(#arrow-muted)}.edge-dash{stroke:#687983;stroke-width:1.5;stroke-dasharray:7 6;fill:none;marker-end:url(#arrow-muted)}
</style>"""
        )
        self.rect(0, 0, width, height, fill="#f5f2eb")
        self.rect(0, 0, width, 82, fill="#102f40")
        self.text(36, 43, title, cls="title inverse", anchor="start")
        self.text(width - 36, 32, code, cls="label inverse", anchor="end")
        self.text(
            width - 36, 57, f"UAF 1.3 / SysML 1.6 | v{VERSION}", cls="small inverse", anchor="end"
        )

    def rect(
        self,
        x: float,
        y: float,
        w: float,
        h: float,
        *,
        fill: str = "#ffffff",
        stroke: str = "none",
        rx: float = 0,
        sw: float = 1,
        opacity: float = 1,
        shadow: bool = False,
    ) -> None:
        filt = ' filter="url(#shadow)"' if shadow else ""
        self.parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" opacity="{opacity}"{filt}/>'
        )

    def line(self, x1: float, y1: float, x2: float, y2: float, *, cls: str = "edge") -> None:
        self.parts.append(f'<path d="M {x1} {y1} L {x2} {y2}" class="{cls}"/>')

    def path(self, d: str, *, cls: str = "edge") -> None:
        self.parts.append(f'<path d="{d}" class="{cls}"/>')

    def text(
        self, x: float, y: float, value: str, *, cls: str = "body", anchor: str = "start"
    ) -> None:
        self.parts.append(
            f'<text x="{x}" y="{y}" class="{cls}" text-anchor="{anchor}">{html.escape(value)}</text>'
        )

    def multiline(
        self,
        x: float,
        y: float,
        value: str,
        *,
        width: int = 34,
        cls: str = "body",
        leading: int = 18,
        anchor: str = "start",
        max_lines: int | None = None,
    ) -> None:
        lines = textwrap.wrap(
            value, width=width, break_long_words=False, break_on_hyphens=False
        ) or [""]
        if max_lines and len(lines) > max_lines:
            lines = lines[:max_lines]
            lines[-1] = lines[-1].rstrip(" .") + "..."
        self.parts.append(f'<text x="{x}" y="{y}" class="{cls}" text-anchor="{anchor}">')
        for index, line in enumerate(lines):
            dy = 0 if index == 0 else leading
            self.parts.append(f'<tspan x="{x}" dy="{dy}">{html.escape(line)}</tspan>')
        self.parts.append("</text>")

    def section(
        self,
        x: float,
        y: float,
        w: float,
        h: float,
        title: str,
        subtitle: str = "",
        *,
        fill: str = "#e8eef0",
    ) -> None:
        self.rect(x, y, w, h, fill=fill, stroke="#b9c8ce", rx=12, sw=1.2)
        self.text(x + 18, y + 26, title, cls="label")
        if subtitle:
            self.text(x + w - 18, y + 26, subtitle, cls="small", anchor="end")

    def card(
        self,
        x: float,
        y: float,
        w: float,
        h: float,
        ident: str,
        title: str,
        body: str = "",
        *,
        fill: str = "#ffffff",
        accent: str = "#2c8580",
        title_size: str = "node-title",
        body_width: int = 34,
        max_lines: int = 3,
    ) -> None:
        self.rect(x, y, w, h, fill=fill, stroke="#b8c7cd", rx=9, sw=1, shadow=True)
        self.rect(x, y, 7, h, fill=accent, rx=7)
        self.text(x + 18, y + 23, ident, cls="node-id")
        self.multiline(
            x + 18, y + 47, title, width=body_width, cls=title_size, leading=18, max_lines=2
        )
        if body:
            body_y = y + (84 if len(title) > body_width else 70)
            self.multiline(
                x + 18,
                body_y,
                body,
                width=body_width + 4,
                cls="small",
                leading=16,
                max_lines=max_lines,
            )

    def footer(self) -> None:
        self.rect(28, self.height - 35, self.width - 56, 1.2, fill="#687983")
        self.text(
            30, self.height - 14, "Veoveo reference baseline - no client-specific data", cls="tiny"
        )
        self.text(
            self.width - 30,
            self.height - 14,
            f"Revision {REVISION_DATE} | {self.code}",
            cls="tiny",
            anchor="end",
        )

    def save(self, filename: str) -> None:
        self.footer()
        self.parts.append("</svg>\n")
        (DIAGRAMS / filename).write_text("\n".join(self.parts), encoding="utf-8")


def diagram_view_map() -> None:
    svg = Svg(
        1700,
        1120,
        "Architecture view map",
        "The UAF domains, SysML detail model, and published Veoveo views with their trace path.",
        "VV-VIEW-00",
    )
    stages = [
        ("Strategic", "Capabilities and outcomes", "VV-CAP-*", "#d9ebe7"),
        ("Operational", "Activities and mission threads", "VV-OPA-*", "#deebf2"),
        ("Services", "Implementation-independent offers", "VV-SVC-*", "#eee5d8"),
        ("Resources", "Software, interfaces, data", "VV-CMP-* / VV-IF-*", "#e7e1ef"),
        ("Actual Resources", "Fielded configurations", "VV-AR-*", "#e9e7db"),
        ("Evidence", "Requirements and verification", "VV-REQ-*", "#f0dfdc"),
    ]
    x = 42
    for index, (name, body, ident, fill) in enumerate(stages):
        svg.card(x, 130, 240, 145, ident, name, body, fill=fill, body_width=24, max_lines=3)
        if index < len(stages) - 1:
            svg.line(x + 240, 202, x + 275, 202)
        x += 275
    svg.text(42, 320, "Governing UAF domains and cross-cutting aspects", cls="label")
    cross = [
        (
            "Architecture Management",
            "Scope, stakeholders, principles, dictionary, model governance",
        ),
        ("Security", "Risks, assets, controls, trust boundaries, identity and audit"),
        ("Standards", "MCP, OAuth/OIDC, UAF, SysML, OCI, OTLP, storage and transport contracts"),
        (
            "Parameters",
            "Limits, recovery semantics, retention, measures, and deployment constraints",
        ),
    ]
    for index, (name, body) in enumerate(cross):
        cx = 42 + index * 407
        svg.card(
            cx,
            345,
            375,
            120,
            f"CROSS-{index + 1}",
            name,
            body,
            fill="#ffffff",
            body_width=42,
            max_lines=2,
        )
    views = [
        ("VV-VIEW-01", "Enterprise context", "Stakeholders and boundary"),
        ("VV-VIEW-02", "Capability map", "Strategic taxonomy"),
        ("VV-VIEW-03", "Operational mission thread", "Operational sequence/process"),
        ("VV-VIEW-04", "Service structure", "Services taxonomy/structure"),
        ("VV-VIEW-05", "Software resource structure", "Resource taxonomy/structure"),
        ("VV-VIEW-06", "Software connectivity", "Resource connectivity"),
        ("VV-VIEW-07", "Deployment configurations", "Actual Resource structure"),
        ("VV-VIEW-08", "Security and trust", "Security structure/traceability"),
        ("VV-VIEW-09", "Requirements traceability", "Traceability and evidence"),
        ("VV-VIEW-10", "Durable task lifecycle", "SysML state detail"),
    ]
    svg.text(42, 520, "Published view set", cls="label")
    for index, (ident, name, concern) in enumerate(views):
        col = index % 5
        row = index // 5
        svg.card(
            42 + col * 327,
            545 + row * 205,
            295,
            165,
            ident,
            name,
            concern,
            fill="#ffffff",
            body_width=30,
            max_lines=2,
        )
    svg.rect(42, 970, 1615, 76, fill="#102f40", rx=10)
    svg.text(65, 1001, "Canonical trace", cls="label inverse")
    svg.text(
        65,
        1028,
        "Requirement -> Capability -> Operational Activity -> Service -> Function -> Software Block -> Actual Resource -> Verification Evidence",
        cls="body inverse",
    )
    svg.save("00-architecture-view-map.svg")


def diagram_context() -> None:
    svg = Svg(
        1700,
        1080,
        "Enterprise context",
        "Veoveo system boundary, external performers, installation-owned dependencies, and principal exchanges.",
        "VV-VIEW-01",
    )
    svg.section(
        315, 125, 1070, 815, "VEOVEO INSTALLATION BOUNDARY", "Organization owned", fill="#e7eef0"
    )
    svg.card(
        610,
        185,
        470,
        130,
        "VV-CMP-040",
        "Installation edge / ingress",
        "Publishes console, gateway, public-share, and required provider routes.",
        fill="#ffffff",
        body_width=50,
    )
    svg.card(
        390,
        365,
        390,
        165,
        "VV-CMP-001",
        "MCP gateway",
        "OAuth, profiles, complete MCP projection, policy, audit, internal identity, admin proxy.",
        fill="#d9ebe7",
        body_width=40,
    )
    svg.card(
        920,
        365,
        390,
        165,
        "VV-CMP-002/003",
        "Operations console",
        "Secure browser session boundary and operational application.",
        fill="#deebf2",
        body_width=40,
    )
    svg.card(
        390,
        600,
        390,
        180,
        "VV-CMP-016..027/049/050/054",
        "Hosted domain servers",
        "Artifact, frames, analytics, media, map, perception, recording, time, view, UAV simulation, and other governed domains.",
        fill="#eee5d8",
        body_width=40,
    )
    svg.card(
        920,
        600,
        390,
        180,
        "VV-CMP-004/005/038/039",
        "Durable data and evidence plane",
        "Artifact service and bytes, Recording Hub and RRD, SurrealDB coordination authority.",
        fill="#e7e1ef",
        body_width=40,
    )
    svg.card(
        610,
        830,
        470,
        80,
        "VV-CMP-006/007",
        "Autonomous agents",
        "Bounded episodes use governed MCP and persist wakes, work, and evidence.",
        fill="#ffffff",
        body_width=52,
        max_lines=2,
    )
    actors = [
        (35, 180, "Operator / architect", "Browser administration and formal review"),
        (35, 430, "MCP client / agent", "Profile-scoped tools, resources, prompts, tasks"),
        (
            35,
            715,
            "Sensor + recording forwarder",
            "Producer-loopback Rerun and OAuth-authenticated gateway upload",
        ),
        (1415, 180, "Enterprise IdP", "OIDC identity, claims, keys"),
        (1415, 430, "Media provider", "Submission API and signed terminal webhook"),
        (1415, 715, "SIEM / operations", "Approved telemetry and audit evidence"),
    ]
    for index, (x, y, name, body) in enumerate(actors):
        svg.card(
            x,
            y,
            250,
            135,
            f"EXT-{index + 1}",
            name,
            body,
            fill="#ffffff",
            body_width=26,
            max_lines=3,
        )
    svg.path("M 285 245 L 560 245 L 560 250 L 610 250")
    svg.path("M 285 495 L 350 495 L 350 447 L 390 447")
    svg.path("M 285 780 L 335 780 L 335 447 L 390 447", cls="edge-muted")
    svg.path("M 1415 245 L 1160 245 L 1160 415 L 780 415")
    svg.path("M 1415 495 L 1360 495 L 1360 690 L 780 690")
    svg.path("M 1415 780 L 1360 780 L 1360 720 L 1310 720", cls="edge-muted")
    svg.line(845, 315, 585, 365)
    svg.line(980, 315, 1115, 365)
    svg.line(585, 530, 585, 600)
    svg.line(1115, 530, 1115, 600)
    svg.line(780, 690, 920, 690)
    svg.path("M 780 447 L 850 447 L 850 690 L 920 690")
    svg.line(845, 830, 585, 780, cls="edge-muted")
    svg.save("01-enterprise-context.svg")


def diagram_capabilities() -> None:
    svg = Svg(
        1700,
        1180,
        "Capability map",
        "Strategic capability taxonomy for the Veoveo reference architecture.",
        "VV-VIEW-02",
    )
    groups = [
        ("Mission-facing capabilities", CAPABILITIES[0:4], "#d9ebe7"),
        ("Domain decision capabilities", CAPABILITIES[4:7], "#deebf2"),
        ("Enterprise platform capabilities", CAPABILITIES[7:10], "#eee5d8"),
        ("Extension and logistics capabilities", CAPABILITIES[10:12], "#e7e1ef"),
    ]
    y = 120
    for group_name, items, fill in groups:
        height = 225 if len(items) >= 3 else 210
        svg.section(40, y, 1620, height, group_name, f"{len(items)} capabilities", fill=fill)
        count = len(items)
        gap = 22
        card_w = (1580 - gap * (count - 1)) / count
        for index, (ident, name, body) in enumerate(items):
            svg.card(
                60 + index * (card_w + gap),
                y + 48,
                card_w,
                height - 68,
                ident,
                name,
                body,
                fill="#ffffff",
                body_width=max(26, int(card_w / 10)),
                max_lines=4,
            )
        y += height + 22
    svg.rect(40, 1045, 1620, 70, fill="#102f40", rx=10)
    svg.text(62, 1075, "Capability realization rule", cls="label inverse")
    svg.text(
        62,
        1101,
        "Every capability traces through at least one operational activity and service to named software resources and verification evidence.",
        cls="body inverse",
    )
    svg.save("02-capability-map.svg")


def diagram_mission_thread() -> None:
    svg = Svg(
        1800,
        1080,
        "Operational mission thread",
        "A representative governed observe-analyze-decide-act thread across Veoveo operational performers.",
        "VV-VIEW-03",
    )
    lanes = [
        ("Mission operator / autonomous agent", "Intent, constraints, decision, authority"),
        ("Gateway and policy boundary", "Identity, profile, policy, audit, routing"),
        ("Domain services", "Recording, map, frames, analytics, optimization, perception"),
        ("Durable work and evidence", "Tasks, outbox, artifacts, RRD recordings"),
    ]
    lane_y = [135, 335, 535, 735]
    for (name, detail), y in zip(lanes, lane_y, strict=True):
        svg.rect(35, y, 1730, 155, fill="#ffffff" if y % 400 else "#edf1f2", stroke="#c1cdd2", rx=8)
        svg.text(55, y + 30, name, cls="label")
        svg.text(55, y + 54, detail, cls="small")
    steps = [
        ("1", "State intent", 250, lane_y[0]),
        ("2", "Authorize", 470, lane_y[1]),
        ("3", "Discover", 690, lane_y[1]),
        ("4", "Observe", 910, lane_y[2]),
        ("5", "Analyze / route", 1130, lane_y[2]),
        ("6", "Plan / decide", 1350, lane_y[0]),
        ("7", "Act", 1570, lane_y[0]),
    ]
    centers: list[tuple[float, float]] = []
    for ident, label, x, y in steps:
        cy = y + 102
        svg.rect(x - 72, cy - 34, 144, 68, fill="#d9ebe7", stroke="#88aca8", rx=34)
        svg.text(x, cy - 6, ident, cls="node-id", anchor="middle")
        svg.text(x, cy + 18, label, cls="body", anchor="middle")
        centers.append((x, cy))
    for (x1, y1), (x2, y2) in zip(centers, centers[1:], strict=False):
        mid = (x1 + x2) / 2
        svg.path(f"M {x1 + 72} {y1} L {mid} {y1} L {mid} {y2} L {x2 - 72} {y2}")
    evidence = [
        (330, "Recording identity", "recording://..."),
        (615, "Policy decision", "principal + target + outcome"),
        (900, "Task identity", "UUIDv7 + recovery class"),
        (1185, "Result identity", "artifact://... / map://..."),
        (1470, "Audit evidence", "method + reason + context"),
    ]
    for x, title, body in evidence:
        svg.card(
            x - 125,
            790,
            250,
            105,
            "EVIDENCE",
            title,
            body,
            fill="#eee5d8",
            body_width=25,
            max_lines=2,
        )
    svg.path("M 910 637 L 910 790", cls="edge-muted")
    svg.path("M 1130 637 L 1185 790", cls="edge-muted")
    svg.path("M 1570 237 L 1570 725 L 1470 790", cls="edge-muted")
    svg.rect(35, 930, 1730, 82, fill="#102f40", rx=10)
    svg.text(58, 962, "Operational invariant", cls="label inverse")
    svg.text(
        58,
        990,
        "Every transition carries canonical identity and policy context; durable work and outputs remain addressable after the initiating episode ends.",
        cls="body inverse",
    )
    svg.save("03-operational-mission-thread.svg")


def diagram_services() -> None:
    svg = Svg(
        1800,
        1200,
        "Service structure",
        "Implementation-independent UAF services and their realization by Veoveo software resources.",
        "VV-VIEW-04",
    )
    tiers = [
        ("Access and oversight", SERVICES[0:2] + SERVICES[7:8], "#d9ebe7"),
        ("Coordination and governance", SERVICES[2:5] + SERVICES[9:10], "#deebf2"),
        ("Domain execution", SERVICES[5:7] + SERVICES[10:12], "#eee5d8"),
        ("Deployment and assurance", SERVICES[8:9], "#e7e1ef"),
    ]
    y = 120
    for tier_name, items, fill in tiers:
        h = 225 if len(items) > 1 else 185
        svg.section(38, y, 1724, h, tier_name, "UAF Services", fill=fill)
        gap = 20
        count = len(items)
        card_w = min(405, (1684 - gap * (count - 1)) / count)
        total = count * card_w + (count - 1) * gap
        start_x = 58 + max(0, (1684 - total) / 2)
        for index, (ident, name, body) in enumerate(items):
            svg.card(
                start_x + index * (card_w + gap),
                y + 47,
                card_w,
                h - 67,
                ident,
                name,
                body,
                fill="#ffffff",
                body_width=max(30, int(card_w / 10)),
                max_lines=4,
            )
        y += h + 22
    mappings = [
        ("VV-SVC-001/002", "VV-CMP-001, 011, 041, 052"),
        ("VV-SVC-003", "VV-CMP-008, 009, 012, 038"),
        ("VV-SVC-004/005", "VV-CMP-001, 004, 005, 014-016, 023, 039, 047, 051, 053"),
        ("VV-SVC-006/011/012", "VV-CMP-017-022, 024-026, 031, 032, 044-046, 049, 050, 054-056"),
        ("VV-SVC-007/008/009/010", "VV-CMP-002, 003, 006, 007, 033-037, 040, 042, 048"),
    ]
    svg.text(42, 1080, "Principal realization allocations", cls="label")
    for index, (svc, comps) in enumerate(mappings):
        x = 42 + index * 345
        svg.text(x, 1110, svc, cls="node-id")
        svg.multiline(x, 1133, comps, width=38, cls="small", leading=15, max_lines=2)
    svg.save("04-service-structure.svg")


def component_by_id(ident: str) -> dict[str, str]:
    return next(component for component in COMPONENTS if component["model_id"] == ident)


def diagram_resource_structure() -> None:
    svg = Svg(
        2200,
        1900,
        "Software resource structure",
        "Complete scoped software component inventory grouped by architectural responsibility.",
        "VV-VIEW-05",
    )
    groups = [
        ("User and control surfaces", ["VV-CMP-001", "VV-CMP-002", "VV-CMP-003"], "#d9ebe7", 4),
        (
            "Core platform libraries and services",
            [f"VV-CMP-{i:03d}" for i in range(4, 16)] + ["VV-CMP-051", "VV-CMP-052", "VV-CMP-053"],
            "#deebf2",
            5,
        ),
        (
            "Hosted MCP domains",
            [f"VV-CMP-{i:03d}" for i in range(16, 28)] + ["VV-CMP-049", "VV-CMP-050", "VV-CMP-054"],
            "#eee5d8",
            4,
        ),
        (
            "Showcase and internal executors",
            [f"VV-CMP-{i:03d}" for i in range(28, 33)] + ["VV-CMP-055"],
            "#e7e1ef",
            6,
        ),
        ("Verification and deployment", [f"VV-CMP-{i:03d}" for i in range(33, 38)], "#e9e7db", 5),
        (
            "External platform and execution resources",
            [f"VV-CMP-{i:03d}" for i in range(38, 49)] + ["VV-CMP-056"],
            "#f0dfdc",
            4,
        ),
    ]
    y = 110
    for group_name, ids, fill, columns in groups:
        rows = (len(ids) + columns - 1) // columns
        section_h = 58 + rows * 92 + 18
        svg.section(30, y, 2140, section_h, group_name, f"{len(ids)} components", fill=fill)
        gap = 18
        card_w = (2100 - gap * (columns - 1)) / columns
        for index, ident in enumerate(ids):
            component = component_by_id(ident)
            col = index % columns
            row = index // columns
            x = 50 + col * (card_w + gap)
            cy = y + 48 + row * 92
            svg.rect(x, cy, card_w, 76, fill="#ffffff", stroke="#b8c7cd", rx=7)
            svg.rect(x, cy, 6, 76, fill="#2c8580", rx=6)
            svg.text(x + 16, cy + 21, ident, cls="node-id")
            svg.multiline(
                x + 16,
                cy + 46,
                component["name"],
                width=max(26, int(card_w / 9)),
                cls="body",
                leading=16,
                max_lines=2,
            )
            svg.text(
                x + card_w - 12, cy + 21, component["implementation"], cls="tiny", anchor="end"
            )
        y += section_h + 18
    svg.save("05-software-resource-structure.svg")


def diagram_connectivity() -> None:
    svg = Svg(
        1900,
        1220,
        "Software resource connectivity",
        "Canonical runtime connections, trust transitions, and authoritative stores.",
        "VV-VIEW-06",
    )
    svg.section(30, 120, 260, 1020, "External zone", "Untrusted / enterprise", fill="#f0dfdc")
    for i, (ident, title) in enumerate(
        [
            ("EXT-1", "Browser / operator"),
            ("EXT-2", "MCP / OAuth client"),
            ("VV-CMP-041", "Enterprise IdP"),
            ("VV-CMP-043", "Media provider"),
            ("EXT-5 / VV-CMP-051", "Sensor + recording forwarder"),
            ("EXT-6", "SIEM / telemetry"),
        ]
    ):
        svg.card(52, 175 + i * 145, 216, 108, ident, title, "", fill="#ffffff", body_width=22)
    svg.section(330, 120, 310, 1020, "Ingress zone", "Published HTTPS", fill="#e9e7db")
    svg.card(
        360,
        190,
        250,
        120,
        "VV-CMP-040",
        "Edge / ingress",
        "TLS and published route boundary.",
        fill="#ffffff",
        body_width=26,
    )
    svg.card(
        360,
        390,
        250,
        150,
        "VV-CMP-002/003",
        "Console BFF + web",
        "Browser login, sessions, CSRF, safe projections.",
        fill="#ffffff",
        body_width=26,
    )
    svg.card(
        360,
        650,
        250,
        165,
        "VV-CMP-001",
        "MCP gateway",
        "OAuth, policy, audit, aggregation, internal identity.",
        fill="#d9ebe7",
        body_width=26,
    )
    svg.card(
        360,
        930,
        250,
        120,
        "VV-CMP-004",
        "Public share redemption",
        "Hash-only bearer resolution.",
        fill="#ffffff",
        body_width=26,
    )
    svg.section(680, 120, 570, 1020, "Application zone", "Gateway-authenticated", fill="#deebf2")
    hosted = [
        ("VV-CMP-016", "Artifact"),
        ("VV-CMP-017", "Frames"),
        ("VV-CMP-018", "DuckDB"),
        ("VV-CMP-019", "Media"),
        ("VV-CMP-020", "Map"),
        ("VV-CMP-021", "Optimization"),
        ("VV-CMP-022", "Perception"),
        ("VV-CMP-023", "Recording"),
        ("VV-CMP-024", "Timeseries"),
        ("VV-CMP-025", "Datasheet"),
        ("VV-CMP-026", "Charts"),
        ("VV-CMP-013/027", "Rerun bridge"),
        ("VV-CMP-049", "Time"),
        ("VV-CMP-050", "View"),
        ("VV-CMP-054", "UAV Simulation"),
    ]
    for index, (ident, title) in enumerate(hosted):
        col, row = index % 4, index // 4
        svg.card(
            700 + col * 132,
            175 + row * 125,
            122,
            92,
            ident,
            title,
            "",
            fill="#ffffff",
            body_width=13,
        )
    svg.card(
        705,
        790,
        250,
        135,
        "VV-CMP-004",
        "Artifact service",
        "Byte policy enforcement and capability redemption.",
        fill="#eee5d8",
        body_width=26,
    )
    svg.card(
        975,
        790,
        250,
        135,
        "VV-CMP-005",
        "Recording Hub",
        "Authorized batch journal and immutable RRD materialization.",
        fill="#eee5d8",
        body_width=26,
    )
    svg.card(
        705,
        965,
        520,
        120,
        "VV-CMP-006/007",
        "Agent kernel and durable runtime",
        "Profile-scoped MCP client with restart-safe scheduling and evidence.",
        fill="#ffffff",
        body_width=55,
    )
    svg.section(
        1290, 120, 580, 1020, "Data and execution zone", "Private authorities", fill="#e7e1ef"
    )
    resources = [
        ("VV-CMP-038", "SurrealDB 3.2", "Platform coordination authority"),
        ("VV-CMP-039", "S3 / RustFS", "Artifact bytes"),
        ("VV-CMP-047", "RRD volumes", "World and episode evidence"),
        ("VV-CMP-045", "DuckDB + Spatial", "Owner analytical workspaces"),
        ("VV-CMP-046", "Map toolchain", "Routing and acquisition"),
        ("VV-CMP-044", "DeepStream / TensorRT", "Local GPU inference"),
        ("VV-CMP-056", "UAV simulator stack", "Isaac, Cesium, Pegasus, and PX4"),
        ("VV-CMP-042", "OTel collector", "Approved operational signals"),
    ]
    for index, (ident, title, body) in enumerate(resources):
        col, row = index % 2, index // 2
        svg.card(
            1318 + col * 267,
            175 + row * 220,
            245,
            165,
            ident,
            title,
            body,
            fill="#ffffff",
            body_width=26,
            max_lines=3,
        )
    svg.path("M 268 230 L 360 450")
    svg.path("M 268 375 L 360 730")
    svg.path("M 268 520 L 360 730")
    svg.path("M 268 665 L 705 392")
    svg.path("M 268 810 L 360 730", cls="edge-muted")
    svg.path("M 268 955 L 1590 945", cls="edge-muted")
    svg.line(485, 310, 485, 390)
    svg.line(485, 540, 485, 650)
    svg.path("M 610 730 L 705 500")
    svg.path("M 610 730 L 975 855")
    svg.path("M 955 855 L 1318 257")
    svg.path("M 955 855 L 1585 257")
    svg.path("M 1098 855 L 1318 477")
    svg.path("M 1225 500 L 1318 697", cls="edge-muted")
    svg.path("M 1225 645 L 1585 697", cls="edge-muted")
    svg.path("M 1130 500 L 1585 477", cls="edge-muted")
    svg.save("06-software-resource-connectivity.svg")


def diagram_deployment() -> None:
    svg = Svg(
        1800,
        1100,
        "Deployment configurations",
        "Three actual resource configurations instantiated from the same Helm service graph.",
        "VV-VIEW-07",
    )
    configs = [
        (
            "VV-AR-001",
            "k3d local",
            "Local Kubernetes",
            "Loopback ingress; development Secrets; mandatory NVIDIA GPU access",
            "#d9ebe7",
        ),
        (
            "VV-AR-002",
            "Kubernetes connected",
            "Kubernetes",
            "External OIDC/S3; Ingress; default-deny NetworkPolicy; mesh mTLS",
            "#eee5d8",
        ),
        (
            "VV-AR-003",
            "Kubernetes offline",
            "Isolated Kubernetes",
            "Imported images; existing Secrets; offline values; retained evidence",
            "#e7e1ef",
        ),
    ]
    for index, (ident, name, form, detail, fill) in enumerate(configs):
        x = 38 + index * 573
        svg.card(
            x,
            135,
            540,
            190,
            ident,
            name,
            f"{form}. {detail}",
            fill=fill,
            body_width=42,
            max_lines=5,
        )
        invariants = [
            "Gateway + console",
            "Hosted MCP domains",
            "Artifact plane",
            "Recording plane",
            "SurrealDB authority",
            "Telemetry + audit",
        ]
        for row, invariant in enumerate(invariants):
            svg.rect(x, 365 + row * 72, 540, 54, fill="#ffffff", stroke="#c0ccd1", rx=6)
            svg.text(x + 18, 398 + row * 72, invariant, cls="body")
            svg.text(x + 515, 398 + row * 72, "INVARIANT", cls="tiny", anchor="end")
        svg.rect(x, 825, 540, 112, fill="#ffffff", stroke="#c0ccd1", rx=8)
        svg.text(x + 18, 855, "Form-specific controls", cls="node-id")
        form_controls = [
            "k3d ports, image import, GPU node",
            "Kubernetes Secrets, PVCs, Ingress",
            "Offline Helm values and image policy",
        ][index]
        svg.multiline(x + 18, 883, form_controls, width=43, cls="body", leading=18, max_lines=3)
    svg.rect(38, 980, 1719, 60, fill="#102f40", rx=10)
    svg.text(
        60,
        1017,
        "Deployment invariant: one enterprise installation owns identity mapping, policy, persistence, artifacts, recordings, telemetry, secrets, ingress, and lifecycle.",
        cls="body inverse",
    )
    svg.save("07-deployment-configurations.svg")


def diagram_security() -> None:
    svg = Svg(
        1900,
        1240,
        "Security and trust",
        "Trust zones, identity transitions, data authorities, and principal security controls.",
        "VV-VIEW-08",
    )
    zones = [
        (
            40,
            130,
            330,
            980,
            "External trust",
            "Enterprise users, clients, IdP, provider, link holders",
            "#f0dfdc",
        ),
        (
            405,
            130,
            330,
            980,
            "Published ingress",
            "TLS edge, console BFF, gateway, public redemption",
            "#e9e7db",
        ),
        (
            770,
            130,
            520,
            980,
            "Authenticated application",
            "Gateway-issued internal identity and server-local authorization",
            "#d9ebe7",
        ),
        (
            1325,
            130,
            535,
            980,
            "Protected data / execution",
            "SurrealDB, object bytes, RRD, analytical workspaces, GPU",
            "#e7e1ef",
        ),
    ]
    for x, y, w, h, name, body, fill in zones:
        svg.section(x, y, w, h, name, "Trust zone", fill=fill)
        svg.multiline(
            x + 18, y + 52, body, width=max(28, int(w / 10)), cls="small", leading=16, max_lines=3
        )
    external = [
        "OIDC principal",
        "OAuth client",
        "Signed provider",
        "Link bearer",
        "Recording producer",
    ]
    for i, name in enumerate(external):
        svg.card(
            70, 245 + i * 150, 270, 100, f"EXT-{i + 1}", name, "", fill="#ffffff", body_width=28
        )
    ingress = [
        ("VV-CMP-040", "Edge controls", "TLS, route allowlist, bearer-log suppression"),
        ("VV-CMP-002", "Browser controls", "PKCE, encrypted cookie, CSRF, CSP"),
        ("VV-CMP-001", "Gateway controls", "issuer/key checks, policy, audit, replay defense"),
        ("VV-CMP-004", "Share controls", "hash-only bearer, expiry, limits, revocation"),
    ]
    for i, (ident, name, body) in enumerate(ingress):
        svg.card(
            435,
            235 + i * 205,
            270,
            150,
            ident,
            name,
            body,
            fill="#ffffff",
            body_width=28,
            max_lines=4,
        )
    app = [
        (
            "CTRL-01",
            "Short-lived internal assertion",
            "Ed25519, kid, issuer, audience, principal, tenant, labels, scopes, expiry",
        ),
        (
            "CTRL-02",
            "Server-local enforcement",
            "Canonical audience, domain scope, stable identifiers, bounded bodies and outputs",
        ),
        (
            "CTRL-03",
            "Durable decision evidence",
            "Policy decision, explicit principal attributes, task owner, audit event, outbox order",
        ),
        (
            "CTRL-04",
            "Least-authority background work",
            "Task-bound expiring write capability; no minted background principal",
        ),
    ]
    for i, (ident, name, body) in enumerate(app):
        svg.card(
            805,
            235 + i * 205,
            450,
            150,
            ident,
            name,
            body,
            fill="#ffffff",
            body_width=48,
            max_lines=4,
        )
    data = [
        (
            "VV-CMP-038",
            "Coordination authority",
            "Separate bootstrap/runtime credentials; transactions; tenant identifiers; outbox",
        ),
        (
            "VV-CMP-039",
            "Artifact bytes",
            "Service-only credentials; tenant storage keys; integrity hash; signed redirect",
        ),
        (
            "VV-CMP-047",
            "Recording evidence",
            "OAuth producer; gateway policy; governed catalog; crash-decodable files",
        ),
        (
            "VV-CMP-045/046",
            "Domain workspaces",
            "Owner paths, locked settings, bounded processes, governed input and output",
        ),
        (
            "VV-CMP-044",
            "GPU execution",
            "Mounted approved engine, task process boundary, no remote inference dependency",
        ),
    ]
    for i, (ident, name, body) in enumerate(data):
        svg.card(
            1358,
            205 + i * 175,
            470,
            135,
            ident,
            name,
            body,
            fill="#ffffff",
            body_width=50,
            max_lines=3,
        )
    svg.line(340, 295, 435, 310)
    svg.line(340, 895, 435, 720)
    svg.line(705, 720, 805, 310)
    svg.line(1255, 310, 1358, 272)
    svg.line(1255, 515, 1358, 447)
    svg.line(1255, 720, 1358, 622)
    svg.rect(40, 1145, 1820, 45, fill="#102f40", rx=8)
    svg.text(
        62,
        1174,
        "Fail-closed rule: unknown profiles, servers, methods, resources, task IDs, artifact IDs, issuers, keys, and policy targets are denied.",
        cls="body inverse",
    )
    svg.save("08-security-and-trust.svg")


def diagram_traceability() -> None:
    svg = Svg(
        2400,
        1500,
        "Requirements traceability",
        "Trace from SysML requirements through UAF capabilities, operational activities, services, resources, and verification evidence.",
        "VV-VIEW-09",
    )
    cols = [
        (35, 125, 150, "Requirement"),
        (190, 125, 690, "Requirement statement"),
        (885, 125, 185, "Capability"),
        (1075, 125, 185, "Activity"),
        (1265, 125, 185, "Service"),
        (1455, 125, 535, "Principal resources"),
        (1995, 125, 370, "Verification evidence"),
    ]
    for x, y, w, label in cols:
        svg.rect(x, y, w, 52, fill="#102f40")
        svg.text(x + 10, y + 32, label, cls="small inverse")
    row_h = 68
    for index, req in enumerate(REQUIREMENTS):
        y = 177 + index * row_h
        fill = "#ffffff" if index % 2 == 0 else "#edf1f2"
        for x, _, w, _ in cols:
            svg.rect(x, y, w, row_h, fill=fill, stroke="#c7d1d5", sw=0.7)
        svg.text(45, y + 27, req["requirement_id"], cls="node-id")
        svg.multiline(
            200, y + 23, req["requirement"], width=88, cls="tiny", leading=13, max_lines=4
        )
        svg.text(895, y + 29, req["capability_id"], cls="small")
        svg.text(1085, y + 29, req["operational_activity_id"], cls="small")
        svg.text(1275, y + 29, req["service_id"], cls="small")
        svg.multiline(
            1465,
            y + 23,
            req["component_ids"].replace("|", ", "),
            width=70,
            cls="tiny",
            leading=13,
            max_lines=4,
        )
        svg.multiline(
            2005,
            y + 23,
            req["verification_evidence"],
            width=48,
            cls="tiny",
            leading=13,
            max_lines=4,
        )
    svg.save("09-requirements-traceability.svg")


def diagram_task_lifecycle() -> None:
    svg = Svg(
        1900,
        1180,
        "Durable task lifecycle",
        "SysML state detail for Veoveo durable work, including the webhook-only provider completion branch.",
        "VV-VIEW-10",
    )
    states = {
        "Created": (110, 220, "Persist owner, request, idempotency, recovery class"),
        "Running": (440, 220, "Claim lease, heartbeat, progress, bounded execution"),
        "Input required": (770, 110, "Persist elicitation request and await authorized submission"),
        "Webhook wait": (
            770,
            350,
            "Provider submission and binding committed; signed webhook only",
        ),
        "Completed": (1190, 130, "Persist terminal result and outbox event"),
        "Failed": (1190, 350, "Persist failure and recovery evidence"),
        "Cancelled": (1190, 570, "Local task remains terminal; provider deletion is best effort"),
    }
    positions: dict[str, tuple[float, float]] = {}
    for index, (name, (x, y, body)) in enumerate(states.items()):
        fill = "#d9ebe7" if name in {"Created", "Running"} else "#ffffff"
        if name == "Webhook wait":
            fill = "#eee5d8"
        if name in {"Completed", "Failed", "Cancelled"}:
            fill = "#e7e1ef"
        svg.card(
            x, y, 275, 145, f"STATE-{index + 1}", name, body, fill=fill, body_width=29, max_lines=4
        )
        positions[name] = (x, y)
    svg.line(385, 292, 440, 292)
    svg.path("M 715 255 L 745 255 L 745 182 L 770 182")
    svg.path("M 907 255 L 907 350")
    svg.path("M 1045 182 L 1120 182 L 1120 202 L 1190 202")
    svg.path("M 1045 422 L 1120 422 L 1120 202 L 1190 202")
    svg.line(1045, 422, 1190, 422)
    svg.path("M 715 315 L 760 315 L 760 642 L 1190 642")
    svg.path("M 907 255 L 907 82 L 1275 82 L 1275 130", cls="edge-muted")
    svg.text(402, 275, "claim", cls="tiny")
    svg.text(728, 235, "request input", cls="tiny")
    svg.text(915, 323, "provider submit", cls="tiny")
    svg.text(1080, 402, "signed terminal webhook", cls="tiny")
    svg.text(1070, 620, "cancel", cls="tiny")
    svg.text(1080, 165, "deterministic result", cls="tiny")
    svg.section(
        90,
        790,
        1720,
        300,
        "Recovery classification",
        "One declaration per operation",
        fill="#deebf2",
    )
    recovery = [
        (
            "Resume",
            "Deterministic and side-effect-safe work may be reclaimed after lease expiry.",
            "#d9ebe7",
        ),
        (
            "WebhookWait",
            "A submitted external job waits only for its signed terminal webhook. No status polling exists.",
            "#eee5d8",
        ),
        (
            "InterruptedIndeterminate",
            "Execution may have mutated state; recovery fails the task and never repeats it.",
            "#f0dfdc",
        ),
    ]
    for index, (name, body, fill) in enumerate(recovery):
        svg.card(
            120 + index * 560,
            855,
            510,
            165,
            f"REC-{index + 1}",
            name,
            body,
            fill=fill,
            body_width=54,
            max_lines=4,
        )
    svg.rect(90, 1100, 1720, 38, fill="#102f40", rx=7)
    svg.text(
        110,
        1125,
        "Provider invariant: a late signed webhook may reconcile provider state and billing, but it cannot replace a locally cancelled task result or create outputs.",
        cls="small inverse",
    )
    svg.save("10-durable-task-lifecycle.svg")


def generate_xmi() -> None:
    xmi_ns = "http://www.omg.org/spec/XMI/20131001"
    uml_ns = "http://www.omg.org/spec/UML/20161101"
    sysml_ns = "http://www.omg.org/spec/SysML/20181001/SysML"
    uaf_ns = "http://www.omg.org/spec/UAF/20241101/UAF"
    ET.register_namespace("xmi", xmi_ns)
    ET.register_namespace("uml", uml_ns)
    ET.register_namespace("SysML", sysml_ns)
    ET.register_namespace("UAF", uaf_ns)
    root = ET.Element(f"{{{xmi_ns}}}XMI", {f"{{{xmi_ns}}}version": "2.5.1"})
    model = ET.SubElement(
        root,
        f"{{{uml_ns}}}Model",
        {f"{{{xmi_ns}}}id": "VV-MODEL-001", "name": "Veoveo UAF SysML Reference Architecture"},
    )
    for ident, href in [
        ("VV-PROFILE-UAF", "https://www.omg.org/spec/UAF/20241101/UAF.xml#UAF"),
        ("VV-PROFILE-SYSML", "https://www.omg.org/spec/SysML/20181001/SysML.xmi#SysML"),
    ]:
        application = ET.SubElement(
            model,
            "profileApplication",
            {f"{{{xmi_ns}}}type": "uml:ProfileApplication", f"{{{xmi_ns}}}id": ident},
        )
        ET.SubElement(application, "appliedProfile", {"href": href})

    def package(ident: str, name: str) -> ET.Element:
        return ET.SubElement(
            model,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Package", f"{{{xmi_ns}}}id": ident, "name": name},
        )

    def comment(owner: ET.Element, ident: str, body: str) -> None:
        ET.SubElement(
            owner,
            "ownedComment",
            {f"{{{xmi_ns}}}type": "uml:Comment", f"{{{xmi_ns}}}id": ident, "body": body},
        )

    strategic = package("VV-PKG-STRATEGIC", "Strategic")
    operational = package("VV-PKG-OPERATIONAL", "Operational")
    services_pkg = package("VV-PKG-SERVICES", "Services")
    resources = package("VV-PKG-RESOURCES", "Resources")
    actual = package("VV-PKG-ACTUAL", "Actual Resources")
    requirements_pkg = package("VV-PKG-REQUIREMENTS", "Requirements")
    roles_pkg = package("VV-PKG-ROLES", "Operational Performers and Roles")
    trace_pkg = package("VV-PKG-TRACE", "Traceability")

    for ident, name, body in CAPABILITIES:
        element = ET.SubElement(
            strategic,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Class", f"{{{xmi_ns}}}id": ident, "name": name},
        )
        comment(element, f"{ident}-COMMENT", body)
        ET.SubElement(
            root,
            f"{{{uaf_ns}}}Capability",
            {f"{{{xmi_ns}}}id": f"{ident}-UAF", "base_Class": ident},
        )

    for ident, name, body in ACTIVITIES:
        element = ET.SubElement(
            operational,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Activity", f"{{{xmi_ns}}}id": ident, "name": name},
        )
        comment(element, f"{ident}-COMMENT", body)
        ET.SubElement(
            root,
            f"{{{uaf_ns}}}OperationalActivity",
            {f"{{{xmi_ns}}}id": f"{ident}-UAF", "base_Activity": ident},
        )

    for ident, name, body in SERVICES:
        element = ET.SubElement(
            services_pkg,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Class", f"{{{xmi_ns}}}id": ident, "name": name},
        )
        comment(element, f"{ident}-COMMENT", body)
        ET.SubElement(
            root, f"{{{uaf_ns}}}Service", {f"{{{xmi_ns}}}id": f"{ident}-UAF", "base_Class": ident}
        )

    component_ids: set[str] = set()
    for component in COMPONENTS:
        ident = component["model_id"]
        component_ids.add(ident)
        element = ET.SubElement(
            resources,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Class", f"{{{xmi_ns}}}id": ident, "name": component["name"]},
        )
        comment(element, f"{ident}-COMMENT", component["responsibility"])
        stereotype = component["uaf_type"]
        ET.SubElement(
            root,
            f"{{{uaf_ns}}}{stereotype}",
            {f"{{{xmi_ns}}}id": f"{ident}-UAF", "base_Class": ident},
        )
        ET.SubElement(
            root,
            f"{{{sysml_ns}}}Block",
            {f"{{{xmi_ns}}}id": f"{ident}-SYSML", "base_Class": ident, "isEncapsulated": "true"},
        )

    installation_system_id = "VV-SYS-001"
    installation_system = ET.SubElement(
        resources,
        "packagedElement",
        {
            f"{{{xmi_ns}}}type": "uml:Class",
            f"{{{xmi_ns}}}id": installation_system_id,
            "name": "Veoveo Installation System",
        },
    )
    comment(
        installation_system,
        f"{installation_system_id}-COMMENT",
        "Aggregate resource performer composed from the canonical Veoveo software and infrastructure service graph.",
    )
    ET.SubElement(
        root,
        f"{{{uaf_ns}}}System",
        {f"{{{xmi_ns}}}id": f"{installation_system_id}-UAF", "base_Class": installation_system_id},
    )
    ET.SubElement(
        root,
        f"{{{sysml_ns}}}Block",
        {
            f"{{{xmi_ns}}}id": f"{installation_system_id}-SYSML",
            "base_Class": installation_system_id,
            "isEncapsulated": "true",
        },
    )

    for ident, name, deployment_definition in ACTUAL_RESOURCES:
        instance = ET.SubElement(
            actual,
            "packagedElement",
            {
                f"{{{xmi_ns}}}type": "uml:InstanceSpecification",
                f"{{{xmi_ns}}}id": ident,
                "name": name,
                "classifier": installation_system_id,
            },
        )
        comment(
            instance,
            f"{ident}-COMMENT",
            f"Reference fielded configuration of the Veoveo Installation System, configured by {deployment_definition}.",
        )
        ET.SubElement(
            root,
            f"{{{uaf_ns}}}ActualResource",
            {f"{{{xmi_ns}}}id": f"{ident}-UAF", "base_InstanceSpecification": ident},
        )
        ET.SubElement(
            trace_pkg,
            "packagedElement",
            {
                f"{{{xmi_ns}}}type": "uml:Dependency",
                f"{{{xmi_ns}}}id": f"{ident}-DEPLOY",
                "name": "configuredBy",
                "client": ident,
                "supplier": deployment_definition,
            },
        )

    for ident, name in ROLES:
        element = ET.SubElement(
            roles_pkg,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Class", f"{{{xmi_ns}}}id": ident, "name": name},
        )
        ET.SubElement(
            root,
            f"{{{uaf_ns}}}OperationalPerformer",
            {f"{{{xmi_ns}}}id": f"{ident}-UAF", "base_Class": ident},
        )

    requirement_ids: set[str] = set()
    for req in REQUIREMENTS:
        ident = req["requirement_id"]
        requirement_ids.add(ident)
        element = ET.SubElement(
            requirements_pkg,
            "packagedElement",
            {f"{{{xmi_ns}}}type": "uml:Class", f"{{{xmi_ns}}}id": ident, "name": ident},
        )
        comment(element, f"{ident}-COMMENT", req["requirement"])
        ET.SubElement(
            root,
            f"{{{sysml_ns}}}Requirement",
            {
                f"{{{xmi_ns}}}id": f"{ident}-SYSML",
                "base_Class": ident,
                "id": ident,
                "text": req["requirement"],
            },
        )

    capability_ids = {item[0] for item in CAPABILITIES}
    activity_ids = {item[0] for item in ACTIVITIES}
    service_ids = {item[0] for item in SERVICES}
    role_ids = {item[0] for item in ROLES}
    all_resources = component_ids | role_ids
    for row_index, req in enumerate(REQUIREMENTS, start=1):
        req_id = req["requirement_id"]
        chain = [req["capability_id"], req["operational_activity_id"], req["service_id"]]
        for link_index, (client, supplier) in enumerate(
            zip([req_id] + chain[:-1], chain, strict=True), start=1
        ):
            if (
                client not in requirement_ids | capability_ids | activity_ids | service_ids
                or supplier not in capability_ids | activity_ids | service_ids
            ):
                continue
            dep_id = f"VV-TRACE-{row_index:02d}-{link_index}"
            ET.SubElement(
                trace_pkg,
                "packagedElement",
                {
                    f"{{{xmi_ns}}}type": "uml:Dependency",
                    f"{{{xmi_ns}}}id": dep_id,
                    "name": "tracesTo",
                    "client": client,
                    "supplier": supplier,
                },
            )
        for comp_index, comp_id in enumerate(req["component_ids"].split("|"), start=1):
            if comp_id not in all_resources:
                continue
            dep_id = f"VV-SAT-{row_index:02d}-{comp_index:02d}"
            ET.SubElement(
                trace_pkg,
                "packagedElement",
                {
                    f"{{{xmi_ns}}}type": "uml:Abstraction",
                    f"{{{xmi_ns}}}id": dep_id,
                    "name": "satisfy",
                    "client": comp_id,
                    "supplier": req_id,
                },
            )
            ET.SubElement(
                root,
                f"{{{sysml_ns}}}Satisfy",
                {f"{{{xmi_ns}}}id": f"{dep_id}-SYSML", "base_Abstraction": dep_id},
            )

    ET.indent(root, space="  ")
    ET.ElementTree(root).write(
        MODEL / "veoveo-uaf-sysml.xmi", encoding="utf-8", xml_declaration=True
    )


def table(
    headers: Sequence[tuple[str, str]], rows: Iterable[dict[str, str]], *, table_id: str = ""
) -> str:
    ident = f' id="{table_id}"' if table_id else ""
    parts = [f'<div class="table-wrap"><table{ident}><thead><tr>']
    parts.extend(f"<th>{html.escape(label)}</th>" for _, label in headers)
    parts.append("</tr></thead><tbody>")
    for row in rows:
        search = " ".join(row.get(key, "") for key, _ in headers).lower()
        parts.append(f'<tr data-search="{html.escape(search)}">')
        for key, _ in headers:
            value = row.get(key, "").replace("|", ", ")
            parts.append(f"<td>{html.escape(value)}</td>")
        parts.append("</tr>")
    parts.append("</tbody></table></div>")
    return "".join(parts)


def generate_html() -> None:
    views = [
        (
            "00-architecture-view-map.svg",
            "Architecture view map",
            "Architecture Management / all-domain navigation",
            "Shows how strategic, operational, service, resource, actual-resource, security, standards, and evidence concerns remain one traceable model.",
        ),
        (
            "01-enterprise-context.svg",
            "Enterprise context",
            "Architecture Management Structure",
            "Defines the installation boundary, external performers, organization-owned dependencies, and principal exchanges.",
        ),
        (
            "02-capability-map.svg",
            "Capability map",
            "Strategic Taxonomy and Traceability",
            "Defines the abilities Veoveo contributes to logistics, defense, analytics, autonomy, governance, and deployment.",
        ),
        (
            "03-operational-mission-thread.svg",
            "Operational mission thread",
            "Operational Sequences and Processes",
            "Follows a representative governed observe-analyze-decide-act thread and the evidence created along it.",
        ),
        (
            "04-service-structure.svg",
            "Service structure",
            "Services Taxonomy and Structure",
            "Separates implementation-independent services from their realizing software resources.",
        ),
        (
            "05-software-resource-structure.svg",
            "Software resource structure",
            "Resources Taxonomy and Structure / SysML BDD",
            f"Enumerates all {len(COMPONENTS)} scoped first-party, packaged, and external software resources.",
        ),
        (
            "06-software-resource-connectivity.svg",
            "Software resource connectivity",
            "Resources Connectivity / SysML IBD",
            "Shows runtime protocols, trust transitions, private authorities, and specialized execution boundaries.",
        ),
        (
            "07-deployment-configurations.svg",
            "Deployment configurations",
            "Actual Resources Structure",
            "Compares local k3d with connected and offline Kubernetes configurations that share one Helm service graph.",
        ),
        (
            "08-security-and-trust.svg",
            "Security and trust",
            "Security Structure and Traceability",
            "Locates identity transitions, trust zones, protected assets, and the controls that preserve policy context.",
        ),
        (
            "09-requirements-traceability.svg",
            "Requirements traceability",
            "Traceability / SysML Requirements",
            "Connects formal requirements to UAF capabilities, operational activities, services, resources, and repository evidence.",
        ),
        (
            "10-durable-task-lifecycle.svg",
            "Durable task lifecycle",
            "Resources States / SysML State Machine",
            "Defines the durable state transitions and the webhook-only provider completion branch.",
        ),
    ]
    view_html: list[str] = []
    for index, (filename, title, viewpoint, concern) in enumerate(views):
        view_html.append(
            f'<section class="view" id="view-{index:02d}"><div class="view-heading"><div><p class="eyebrow">VV-VIEW-{index:02d}</p><h2>{html.escape(title)}</h2></div><p class="viewpoint">{html.escape(viewpoint)}</p></div><p>{html.escape(concern)}</p><a class="diagram-link" href="diagrams/{filename}">Open vector view</a><img class="diagram" src="diagrams/{filename}" alt="{html.escape(title)}"></section>'
        )
    component_table = table(
        [
            ("model_id", "ID"),
            ("name", "Component"),
            ("category", "Category"),
            ("implementation", "Implementation"),
            ("deployment", "Deployment"),
            ("responsibility", "Responsibility"),
            ("authoritative_state", "Authority"),
        ],
        COMPONENTS,
        table_id="component-table",
    )
    interface_table = table(
        [
            ("interface_id", "ID"),
            ("name", "Interface"),
            ("source_id", "Source"),
            ("target_id", "Target"),
            ("protocol_or_contract", "Protocol / contract"),
            ("identity_and_policy", "Identity and policy"),
            ("availability_and_recovery", "Availability / recovery"),
        ],
        INTERFACES,
    )
    requirement_table = table(
        [
            ("requirement_id", "ID"),
            ("requirement", "Requirement"),
            ("capability_id", "Capability"),
            ("operational_activity_id", "Activity"),
            ("service_id", "Service"),
            ("component_ids", "Resources"),
            ("verification_evidence", "Evidence"),
        ],
        REQUIREMENTS,
    )
    glossary_table = table(
        [
            ("term", "Term"),
            ("definition", "Definition"),
            ("uaf_or_sysml_mapping", "Formal mapping"),
        ],
        GLOSSARY,
    )
    document = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Veoveo UAF / SysML Reference Architecture</title>
<style>
:root{{--ink:#102f40;--muted:#526a76;--teal:#176b68;--line:#c1cdd2;--paper:#f5f2eb;--panel:#fff;--wash:#e7eef0}}
*{{box-sizing:border-box}}html{{scroll-behavior:smooth}}body{{margin:0;background:var(--paper);color:var(--ink);font-family:Arial,Helvetica,sans-serif;line-height:1.45}}a{{color:var(--teal)}}header{{background:var(--ink);color:#fff;padding:52px clamp(24px,6vw,88px)}}header .kicker,.eyebrow{{font-size:12px;font-weight:700;letter-spacing:.12em;text-transform:uppercase}}header h1{{font-size:clamp(34px,6vw,68px);line-height:1.02;max-width:980px;margin:14px 0 18px}}header .abstract{{max-width:1020px;font-size:19px;color:#e1e9ec}}.meta{{display:flex;gap:28px;flex-wrap:wrap;margin-top:28px;font-size:13px;color:#c8d6db}}nav{{position:sticky;top:0;z-index:3;display:flex;gap:18px;overflow:auto;padding:12px clamp(18px,5vw,72px);background:#fff;border-bottom:1px solid var(--line);white-space:nowrap}}nav a{{font-size:13px;text-decoration:none;color:var(--ink)}}main{{max-width:1480px;margin:auto;padding:42px clamp(18px,4vw,54px) 80px}}section{{scroll-margin-top:70px;margin:0 0 58px}}h2{{font-size:30px;margin:0 0 12px}}h3{{font-size:20px;margin:28px 0 8px}}p{{max-width:1040px}}.summary-grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:16px;margin:24px 0}}.summary{{background:#fff;border:1px solid var(--line);padding:18px;border-radius:8px}}.summary strong{{display:block;font-size:30px}}.notice{{background:#eee5d8;border-left:6px solid #b98a52;padding:18px 22px;margin:24px 0;max-width:1150px}}.view{{background:#fff;border:1px solid var(--line);padding:28px;border-radius:12px;break-before:page}}.view-heading{{display:flex;justify-content:space-between;gap:24px;align-items:start}}.viewpoint{{font-size:13px;color:var(--muted);text-align:right;max-width:380px}}.diagram{{display:block;width:100%;height:auto;margin:20px auto 0;border:1px solid var(--line);background:var(--paper)}}.diagram-link{{font-size:13px}}.table-wrap{{overflow:auto;background:#fff;border:1px solid var(--line);margin:18px 0 36px}}table{{width:100%;border-collapse:collapse;font-size:12px}}th,td{{padding:9px 10px;border-bottom:1px solid #d7dfe2;text-align:left;vertical-align:top}}th{{position:sticky;top:0;background:var(--ink);color:#fff;z-index:1}}tbody tr:nth-child(even){{background:#edf1f2}}code{{font-size:.92em}}.filter{{display:flex;gap:12px;align-items:center;margin:18px 0}}.filter input{{min-width:320px;max-width:100%;padding:10px 12px;border:1px solid var(--line);border-radius:5px;background:#fff;color:var(--ink)}}.doc-control td:first-child{{font-weight:700;width:220px}}footer{{padding:28px clamp(18px,5vw,72px);background:var(--ink);color:#dce6e9;font-size:12px}}
@media(max-width:700px){{.view-heading{{display:block}}.viewpoint{{text-align:left}}.filter{{display:block}}.filter input{{min-width:0;width:100%}}}}
@media print{{@page{{size:A4 landscape;margin:10mm}}body{{background:#fff;font-size:9pt}}nav,.diagram-link,.filter{{display:none}}header{{height:188mm;min-height:188mm;padding:28mm 20mm;background:#102f40!important;break-after:page;-webkit-print-color-adjust:exact;print-color-adjust:exact}}header h1{{font-size:34pt}}main{{max-width:none;padding:0}}section{{margin:0 0 8mm}}#method{{break-before:page;break-inside:avoid}}.view{{height:188mm;border:0;padding:0;break-before:page;break-inside:avoid;overflow:hidden}}.view-heading{{align-items:start}}.view h2{{font-size:18pt;margin:0 0 2mm}}.view .eyebrow{{margin:0 0 1mm}}.view>p{{margin:0 0 2mm}}.diagram{{display:block;width:auto;max-width:100%;height:147mm;object-fit:contain;border:0;margin:2mm auto 0}}.table-wrap{{overflow:visible;border:0}}table{{font-size:6.7pt}}th,td{{padding:4px}}th{{position:static;background:#102f40!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}}tbody tr:nth-child(even){{background:#edf1f2!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}}.catalog-section{{break-before:page}}footer{{display:none}}}}
</style>
</head>
<body>
<header>
<p class="kicker">Reference baseline / no client-specific data</p>
<h1>Veoveo UAF 1.3 / SysML 1.6 Reference Architecture</h1>
<p class="abstract">Veoveo is an organization-owned MCP platform for governed capabilities, durable work, artifacts, recordings, analytical decision support, and autonomous agents. This architecture describes the complete scoped software structure and connects it to enterprise capabilities, operational mission threads, services, security controls, deployment configurations, requirements, and repository evidence.</p>
<div class="meta"><span>Version {VERSION}</span><span>Revision {REVISION_DATE}</span><span>Source commit {SOURCE_COMMIT[:12]}</span><span>{len(COMPONENTS)} software resources</span><span>{len(INTERFACES)} interfaces</span><span>{len(REQUIREMENTS)} requirements</span></div>
</header>
<nav><a href="#control">Control</a><a href="#method">Method</a><a href="#view-00">Views</a><a href="#components">Components</a><a href="#interfaces">Interfaces</a><a href="#requirements">Requirements</a><a href="#glossary">Glossary</a><a href="#references">References</a></nav>
<main>
<section id="control"><h2>Document control</h2>
<table class="doc-control"><tbody><tr><td>Title</td><td>Veoveo UAF 1.3 / SysML 1.6 Reference Architecture</td></tr><tr><td>Architecture identity</td><td>VV-MODEL-001</td></tr><tr><td>Purpose</td><td>Generic reference architecture for formal client review and model exchange.</td></tr><tr><td>Scope</td><td>All 28 Rust workspace packages, the React console, Python SDK and server template, internal Python/C++ executors, deployment and verification components, and external runtime resources in the canonical architecture.</td></tr><tr><td>Exclusions</td><td>Source-code modules within a component, transient permission/init jobs, test fixtures, customer-specific configuration, personnel structure, facilities, and classified mission content.</td></tr><tr><td>Handling</td><td>Reference baseline with no client-specific data. Apply contract, export-control, CUI, distribution, and classification markings before client release.</td></tr><tr><td>Authority</td><td>The repository implementation and executable verification remain product evidence. The model organizes and traces that evidence; it does not replace tests.</td></tr></tbody></table>
<div class="summary-grid"><div class="summary"><strong>{len(CAPABILITIES)}</strong>UAF capabilities</div><div class="summary"><strong>{len(ACTIVITIES)}</strong>operational activities</div><div class="summary"><strong>{len(SERVICES)}</strong>services</div><div class="summary"><strong>{len(COMPONENTS)}</strong>software resources</div><div class="summary"><strong>{len(ACTUAL_RESOURCES)}</strong>actual configurations</div></div></section>
<section id="method"><h2>Architecture method and conformance posture</h2><p>UAF 1.3 governs the enterprise and mission architecture. SysML 1.6 supplies detailed software blocks, requirement semantics, connectivity, and lifecycle behavior beneath it. Stable identifiers preserve traceability across diagrams, catalogs, XMI, repository evidence, and future client overlays.</p>
<div class="notice"><strong>Model-exchange posture.</strong> The included XMI applies the official OMG UAF 1.3 and SysML 1.6 profile URIs and carries the semantic elements and trace relationships used by this report. The release validates XML structure, identifiers, references, catalog coverage, and rendered outputs. Vendor-specific diagram notation and profile OCL validation require import into a UAF 1.3-capable modeling tool; the package does not claim a vendor-native project or tool-certified UAF conformance.</div>
<h3>Trace rule</h3><p><code>Requirement -&gt; Capability -&gt; Operational Activity -&gt; Service -&gt; Software Resource -&gt; Actual Resource -&gt; Verification Evidence</code></p>
<h3>Baseline invariants</h3><p>SurrealDB is the sole platform coordination authority. Provider completion is webhook-only. The gateway preserves the complete MCP surface where it fits. Artifacts, recordings, tasks, agents, policy decisions, and audit evidence retain canonical identities. Helm instantiates the same service graph in local k3d and fielded Kubernetes.</p>
<h3>Selected formal layers</h3><table class="doc-control"><thead><tr><th>Layer</th><th>Question answered</th><th>Veoveo model elements</th></tr></thead><tbody><tr><td>Strategic</td><td>Which enterprise abilities and outcomes matter?</td><td><code>VV-CAP-*</code></td></tr><tr><td>Operational</td><td>Which logical activities and mission threads produce the outcome?</td><td><code>VV-OPA-*</code></td></tr><tr><td>Services</td><td>Which implementation-independent offers support the activities?</td><td><code>VV-SVC-*</code></td></tr><tr><td>Resources</td><td>Which software, interfaces, data, and runtimes realize the services?</td><td><code>VV-CMP-*</code> and <code>VV-IF-*</code></td></tr><tr><td>Actual Resources</td><td>Which connected or offline fielded configuration is present?</td><td><code>VV-AR-*</code></td></tr><tr><td>Requirements and evidence</td><td>Which claims constrain the architecture and where are they verified?</td><td><code>VV-REQ-*</code> and repository evidence</td></tr></tbody></table></section>
{"".join(view_html)}
<section class="catalog-section" id="components"><h2>Software component catalog</h2><p>The catalog includes every buildable first-party component and every external runtime that participates in the stated architecture. Deployment aliases and external operational actors appear in the interface model but are not counted as software resources.</p><div class="filter"><label for="component-filter">Filter components</label><input id="component-filter" type="search" placeholder="ID, name, category, implementation, responsibility"></div>{component_table}</section>
<section class="catalog-section" id="interfaces"><h2>Interface and protocol catalog</h2><p>Interfaces describe canonical ownership, protocol, identity transition, data exchanged, and recovery behavior. Convenience projections do not create alternate authorities.</p>{interface_table}</section>
<section class="catalog-section" id="requirements"><h2>Requirements traceability catalog</h2><p>Requirements are modeled as SysML 1.6 requirements and linked through UAF concepts to software resources and executable repository evidence.</p>{requirement_table}</section>
<section class="catalog-section" id="glossary"><h2>Model glossary</h2>{glossary_table}</section>
<section id="references"><h2>Normative and implementation references</h2><ul><li><a href="https://www.omg.org/spec/UAF/1.3">OMG Unified Architecture Framework 1.3</a>, including the DMM, UAFML, traceability appendix, sample problem, enterprise architecture guide, and normative machine-readable profile.</li><li><a href="https://www.omg.org/spec/SysML/1.6">OMG Systems Modeling Language 1.6</a>.</li><li><a href="../../README.md">Veoveo README</a>, <a href="../ARCHITECTURE_DECISIONS.md">architecture decisions</a>, <a href="../TECH_DESIGN.md">technical design</a>, and <a href="../CODEMAP.md">code map</a>.</li><li><a href="model/veoveo-uaf-sysml.xmi">Veoveo model exchange XMI</a>, <a href="model/model-manifest.yaml">model manifest</a>, and <a href="model/MODEL-IMPORT.md">import guidance</a>.</li></ul></section>
</main>
<footer>Veoveo Reference Architecture {VERSION} | Revision {REVISION_DATE} | VV-MODEL-001 | Reference baseline - no client-specific data</footer>
<script>
const input=document.getElementById('component-filter');const rows=[...document.querySelectorAll('#component-table tbody tr')];input.addEventListener('input',()=>{{const q=input.value.trim().toLowerCase();for(const row of rows)row.hidden=q&&!row.dataset.search.includes(q);}});
</script>
</body></html>"""
    (ARCH / "index.html").write_text(document, encoding="utf-8")


def main() -> None:
    DIAGRAMS.mkdir(parents=True, exist_ok=True)
    MODEL.mkdir(parents=True, exist_ok=True)
    diagram_view_map()
    diagram_context()
    diagram_capabilities()
    diagram_mission_thread()
    diagram_services()
    diagram_resource_structure()
    diagram_connectivity()
    diagram_deployment()
    diagram_security()
    diagram_traceability()
    diagram_task_lifecycle()
    generate_xmi()
    generate_html()


if __name__ == "__main__":
    main()
