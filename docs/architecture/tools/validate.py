#!/usr/bin/env python3
"""Validate the Veoveo reference architecture and its repository coverage."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse

from pypdf import PdfReader

ARCH = Path(__file__).resolve().parents[1]
REPO = ARCH.parents[1]
CATALOGS = ARCH / "catalogs"
MODEL = ARCH / "model" / "veoveo-uaf-sysml.xmi"
XMI_NS = "http://www.omg.org/spec/XMI/20131001"
EXPECTED_TOOLS = {"package.py", "qa.py", "render.py", "validate.py"}


def fail(message: str) -> None:
    raise AssertionError(message)


def rows(name: str) -> list[dict[str, str]]:
    with (CATALOGS / name).open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


class LocalLinkParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.links: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attr = dict(attrs)
        if tag in {"a", "link"} and attr.get("href"):
            self.links.append(attr["href"] or "")
        if tag in {"img", "script"} and attr.get("src"):
            self.links.append(attr["src"] or "")


def unique(values: list[str], label: str) -> None:
    if len(values) != len(set(values)):
        fail(f"duplicate {label}: {values}")


def validate_tool_boundary() -> None:
    tools = {path.name for path in (ARCH / "tools").glob("*.py")}
    if tools != EXPECTED_TOOLS:
        fail(f"generic architecture tool boundary differs: {sorted(tools ^ EXPECTED_TOOLS)}")


def main() -> None:
    validate_tool_boundary()
    components = rows("software-components.csv")
    interfaces = rows("interfaces-and-protocols.csv")
    requirements = rows("requirements-traceability.csv")
    glossary = rows("model-glossary.csv")
    if len(components) != 48:
        fail(f"expected 48 components, found {len(components)}")
    if len(interfaces) != 33:
        fail(f"expected 33 interfaces, found {len(interfaces)}")
    if len(requirements) != 18:
        fail(f"expected 18 requirements, found {len(requirements)}")
    if len(glossary) < 25:
        fail(f"expected at least 25 glossary terms, found {len(glossary)}")

    component_ids = [row["model_id"] for row in components]
    interface_ids = [row["interface_id"] for row in interfaces]
    requirement_ids = [row["requirement_id"] for row in requirements]
    unique(component_ids, "component IDs")
    unique(interface_ids, "interface IDs")
    unique(requirement_ids, "requirement IDs")

    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=REPO,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    workspace_paths = {
        str(Path(package["manifest_path"]).parent.relative_to(REPO))
        for package in metadata["packages"]
    }
    catalog_paths = {row["repository_path"] for row in components}
    missing_workspace_paths = sorted(workspace_paths - catalog_paths)
    if missing_workspace_paths:
        fail(f"workspace packages missing from component catalog: {missing_workspace_paths}")
    if len(workspace_paths) != 26:
        fail(f"expected 26 Rust workspace packages, found {len(workspace_paths)}")

    gateway = json.loads((REPO / "configs/gateway.local.json").read_text(encoding="utf-8"))
    slugs = {server["slug"] for server in gateway["servers"]}
    expected_slugs = {
        "artifact",
        "charts",
        "datasheet",
        "duckdb",
        "frames",
        "map",
        "media",
        "optimization",
        "perception",
        "recording",
        "rerun",
        "timeseries",
    }
    if slugs != expected_slugs:
        fail(f"gateway server identities differ: {sorted(slugs ^ expected_slugs)}")

    tree = ET.parse(MODEL)
    root = tree.getroot()
    xmi_id = f"{{{XMI_NS}}}id"
    xmi_ids = [element.attrib[xmi_id] for element in root.iter() if xmi_id in element.attrib]
    unique(xmi_ids, "XMI IDs")
    xmi_id_set = set(xmi_ids)
    required_model_ids = set(component_ids) | set(requirement_ids)
    required_model_ids |= {f"VV-CAP-{index:03d}" for index in range(1, 13)}
    required_model_ids |= {f"VV-OPA-{index:03d}" for index in range(1, 11)}
    required_model_ids |= {f"VV-SVC-{index:03d}" for index in range(1, 13)}
    required_model_ids |= {f"VV-AR-{index:03d}" for index in range(1, 5)}
    required_model_ids.add("VV-SYS-001")
    missing_model_ids = sorted(required_model_ids - xmi_id_set)
    if missing_model_ids:
        fail(f"catalog identities missing from XMI: {missing_model_ids}")
    for component_id in component_ids:
        if f"{component_id}-UAF" not in xmi_id_set or f"{component_id}-SYSML" not in xmi_id_set:
            fail(f"component lacks UAF/SysML stereotype applications: {component_id}")

    internal_reference_attributes = {
        "base_Class",
        "base_Activity",
        "base_InstanceSpecification",
        "base_Abstraction",
        "client",
        "supplier",
        "classifier",
    }
    for element in root.iter():
        for name, value in element.attrib.items():
            if name in internal_reference_attributes:
                for ref in value.split():
                    if ref not in xmi_id_set:
                        fail(f"unresolved XMI reference {name}={ref}")
    profile_hrefs = {
        element.attrib["href"]
        for element in root.iter()
        if element.tag.endswith("appliedProfile") and "href" in element.attrib
    }
    expected_profiles = {
        "https://www.omg.org/spec/UAF/20241101/UAF.xml#UAF",
        "https://www.omg.org/spec/SysML/20181001/SysML.xmi#SysML",
    }
    if profile_hrefs != expected_profiles:
        fail(f"unexpected applied profile set: {profile_hrefs}")

    role_ids = {
        "VV-EXT-MCP-CLIENT",
        "VV-EXT-BROWSER",
        "VV-EXT-OAUTH-CLIENT",
        "VV-EXT-AUTHORIZED-USER",
        "VV-EXT-LINK-HOLDER",
        "VV-EXT-PRODUCER",
        "VV-CMP-HOSTED",
        "VV-CMP-PLATFORM",
    }
    valid_endpoint_ids = set(component_ids) | role_ids
    for interface in interfaces:
        for field in ("source_id", "target_id"):
            if interface[field] not in valid_endpoint_ids:
                fail(
                    f"unknown interface endpoint {interface[field]} in {interface['interface_id']}"
                )
    valid_requirement_resources = set(component_ids) | {"VV-CMP-HOSTED", "VV-CMP-PLATFORM"}
    for requirement in requirements:
        for resource in requirement["component_ids"].split("|"):
            if resource not in valid_requirement_resources:
                fail(f"unknown requirement resource {resource} in {requirement['requirement_id']}")
        for field in ("capability_id", "operational_activity_id", "service_id"):
            if requirement[field] not in xmi_id_set:
                fail(
                    f"unknown trace identity {requirement[field]} in {requirement['requirement_id']}"
                )

    diagrams = sorted((ARCH / "diagrams").glob("*.svg"))
    if len(diagrams) != 11:
        fail(f"expected 11 SVG views, found {len(diagrams)}")
    for diagram in diagrams:
        ET.parse(diagram)

    parser = LocalLinkParser()
    parser.feed((ARCH / "index.html").read_text(encoding="utf-8"))
    for link in parser.links:
        parsed = urlparse(link)
        if parsed.scheme or link.startswith("#"):
            continue
        local = (ARCH / parsed.path).resolve()
        if not local.exists():
            fail(f"broken local HTML link: {link}")

    pdf = ARCH / "veoveo-reference-architecture.pdf"
    if not pdf.exists() or not pdf.read_bytes().startswith(b"%PDF"):
        fail("formal PDF is missing or invalid")
    page_count = len(PdfReader(pdf).pages)
    if page_count < 20:
        fail("formal PDF page count is unexpectedly low")

    print(
        "validated: "
        f"{len(workspace_paths)} Rust packages, {len(slugs)} gateway servers, "
        f"{len(components)} software resources, {len(interfaces)} interfaces, "
        f"{len(requirements)} requirements, {len(diagrams)} SVG views, "
        f"{page_count} PDF pages"
    )


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, ET.ParseError, subprocess.CalledProcessError) as error:
        print(f"architecture validation failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
