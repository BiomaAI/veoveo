#!/usr/bin/env python3
"""Theme the interactive platform map for the whitepaper.

platform-map.archify.json is an Archify architecture candidate. Archify renders it
as a self-contained interactive viewer with node details, source links, search and
route highlighting:

    node <archify>/bin/archify.mjs finalize architecture \
        docs/images/platform-map.archify.json <viewer>.html \
        --repo-root . --quality showcase --json

This script copies that viewer to platform-map.html with the veoveo.ai night
palette. The palette overrides every Archify theme and visual style, and the
script removes the theme and style controls with their keyboard shortcuts and the
node list beside the diagram. It also
writes platform-map-print.svg, the same diagram with the same palette, for the
printed whitepaper, where a frame cannot be interactive.

Usage:
    uv run --python 3.13 docs/images/platform_map.py <viewer>.html
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUTPUT = os.path.join(HERE, "platform-map.html")
PRINT_OUTPUT = os.path.join(HERE, "platform-map-print.svg")

PALETTE = {
    "--bg": "#020209",
    "--grid": "#120E1C",
    "--canvas-dot": "rgba(201, 163, 230, 0.07)",
    "--text": "#FFFFFF",
    "--text-muted": "#A99DBB",
    "--text-dim": "#5A4E70",
    "--text-faint": "#8C7FA0",
    "--panel": "rgba(20, 12, 32, 0.72)",
    "--panel-border": "#2E2640",
    "--lane-fill": "rgba(20, 12, 32, 0.3)",
    "--lane-stroke": "#3A2F4F",
    "--arrow": "#6E6480",
    "--arrow-emphasis": "#C9A3E6",
    "--mask": "#0B0713",
    "--frontend-fill": "rgba(201, 163, 230, 0.10)",
    "--frontend-stroke": "#C9A3E6",
    "--backend-fill": "rgba(116, 42, 152, 0.20)",
    "--backend-stroke": "#9A5BC4",
    "--database-fill": "rgba(226, 204, 245, 0.08)",
    "--database-stroke": "#E2CCF5",
    "--cloud-fill": "rgba(90, 78, 112, 0.14)",
    "--cloud-stroke": "#6E6480",
    "--security-fill": "rgba(176, 75, 224, 0.18)",
    "--security-stroke": "#B04BE0",
    "--messagebus-fill": "rgba(217, 139, 232, 0.14)",
    "--messagebus-stroke": "#D98BE8",
    "--external-fill": "rgba(2, 2, 9, 0.6)",
    "--external-stroke": "#5A4E70",
    "--toolbar-bg": "rgba(11, 7, 19, 0.88)",
    "--toolbar-border": "#2E2640",
    "--toolbar-text": "#E7E1F0",
    "--toolbar-hover": "rgba(116, 42, 152, 0.28)",
    "--toolbar-menu-bg": "#0B0713",
}

SANS = '"Helvetica Neue", Helvetica, Arial, sans-serif'
THEME = "\n".join([
    '<style id="veoveo-theme">',
    "  html, html[data-theme], html[data-theme][data-preset] {",
    *[f"    {name}: {value} !important;" for name, value in PALETTE.items()],
    "  }",
    f"  body {{ font-family: {SANS} !important; background-image: none !important; }}",
    "  #btn-theme, .preset-wrap, .reader-rail { display: none !important; }",
    "  .container { max-width: none !important; }",
    "  html.veoveo-embedded, html.veoveo-embedded body { overflow: hidden !important; min-height: 0 !important; height: auto !important; }",
    "  html.veoveo-embedded .diagram-container { padding: 14px 14px 68px !important; }",
    "  .diagram-container > svg .c-region { fill: rgba(116, 42, 152, 0.05) !important; stroke: #3A2F4F !important; }",
    "</style>",
    "<script>",
    "  // The palette is fixed: pin Archify's state and drop its theme and style shortcuts.",
    "  document.documentElement.setAttribute('data-theme', 'dark');",
    "  document.documentElement.setAttribute('data-preset', 'classic');",
    "  // Embedded in the whitepaper, the frame takes the content height. The page never scrolls, and",
    "  // fixed padding replaces Archify's viewport-height rules, which would resize the frame in a loop.",
    "  if (window.self !== window.top) document.documentElement.classList.add('veoveo-embedded');",
    "  addEventListener('keydown', (event) => {",
    "    const typing = event.target.closest && event.target.closest('input, textarea, [contenteditable]');",
    "    if (!typing && !event.metaKey && !event.ctrlKey && !event.altKey && /^[tTsS]$/.test(event.key)) {",
    "      event.stopImmediatePropagation();",
    "    }",
    "  }, true);",
    "</script>",
])

# Archify's diagram classes, resolved against PALETTE for a standalone SVG.
PRINT_RULES = """
  svg { font-family: ui-monospace, "SF Mono", SFMono-Regular, Menlo, Consolas, monospace; }
  .c-grid { stroke: transparent; fill: none; }
  .c-mask { fill: var(--mask); stroke: none; }
  .c-frontend { fill: var(--frontend-fill); stroke: var(--frontend-stroke); }
  .c-backend { fill: var(--backend-fill); stroke: var(--backend-stroke); }
  .c-database { fill: var(--database-fill); stroke: var(--database-stroke); }
  .c-cloud { fill: var(--cloud-fill); stroke: var(--cloud-stroke); }
  .c-security { fill: var(--security-fill); stroke: var(--security-stroke); }
  .c-messagebus { fill: var(--messagebus-fill); stroke: var(--messagebus-stroke); }
  .c-external { fill: var(--external-fill); stroke: var(--external-stroke); }
  .c-security-group { fill: transparent; stroke: var(--security-stroke); stroke-dasharray: 4,4; }
  .c-region { fill: rgba(116, 42, 152, 0.05); stroke: #3A2F4F; stroke-dasharray: 8,4; }
  .t-primary { fill: var(--text); }
  .t-muted { fill: var(--text-muted); }
  .t-dim { fill: var(--text-dim); }
  .t-frontend { fill: var(--frontend-stroke); }
  .t-backend { fill: var(--backend-stroke); }
  .t-database { fill: var(--database-stroke); }
  .t-cloud { fill: var(--cloud-stroke); }
  .t-security { fill: var(--security-stroke); }
  .t-external { fill: var(--external-stroke); }
  .t-edge-default { fill: var(--arrow); }
  .t-edge-emphasis { fill: var(--arrow-emphasis); }
  .t-edge-security { fill: var(--security-stroke); }
  .t-edge-dashed { fill: var(--database-stroke); }
  .a-default { stroke: var(--arrow); fill: none; }
  .a-emphasis { stroke: var(--arrow-emphasis); fill: none; }
  .a-security { stroke: var(--security-stroke); fill: none; stroke-dasharray: 5,5; }
  .a-dashed { stroke: var(--database-stroke); fill: none; stroke-dasharray: 4,4; }
  .m-default { fill: var(--arrow); }
  .m-emphasis { fill: var(--arrow-emphasis); }
  .m-security { fill: var(--security-stroke); }
  .m-dashed { fill: var(--database-stroke); }
  .semantic-sigil { fill: none; stroke: currentColor; stroke-width: 1.35; stroke-linecap: round; stroke-linejoin: round; opacity: 0.76; }
  .semantic-sigil .sigil-fill { fill: currentColor; stroke: none; }
  .s-frontend { color: var(--frontend-stroke); }
  .s-backend { color: var(--backend-stroke); }
  .s-database { color: var(--database-stroke); }
  .s-cloud { color: var(--cloud-stroke); }
  .s-security { color: var(--security-stroke); }
  .s-messagebus { color: var(--messagebus-stroke); }
  .s-external { color: var(--external-stroke); }
"""


def print_svg(viewer):
    start = viewer.index("<svg")
    end = viewer.index("</svg>", start) + len("</svg>")
    svg = viewer[start:end]
    head_end = svg.index(">") + 1
    width, height = re.search(r'viewBox="0 0 ([0-9.]+) ([0-9.]+)"', svg[:head_end]).groups()
    variables = " ".join(f"{name}: {value};" for name, value in PALETTE.items())
    opening = svg[:head_end].replace(
        "<svg", f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"', 1)
    style = f"<style>\n  svg {{ {variables} }}{PRINT_RULES}</style>"
    background = f'<rect width="{width}" height="{height}" fill="{PALETTE["--bg"]}"/>'
    return opening + style + background + svg[head_end:] + "\n"


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    viewer = open(sys.argv[1], encoding="utf-8").read()
    if "</head>" not in viewer or 'id="btn-theme"' not in viewer:
        raise SystemExit("input is not an Archify viewer")
    themed = viewer.replace("</head>", THEME + "\n</head>", 1)
    with open(OUTPUT, "w", encoding="utf-8") as handle:
        handle.write(themed)
    print(f"wrote {OUTPUT} ({len(themed) // 1024} KiB)")
    printable = print_svg(viewer)
    with open(PRINT_OUTPUT, "w", encoding="utf-8") as handle:
        handle.write(printable)
    print(f"wrote {PRINT_OUTPUT} ({len(printable) // 1024} KiB)")


if __name__ == "__main__":
    main()
