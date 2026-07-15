#!/usr/bin/env python3
"""Build deterministic client review and model-exchange archives."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ARCH = Path(__file__).resolve().parents[1]
REPO = ARCH.parents[1]
OUTPUT = REPO / "output" / "architecture"
STAGING = REPO / "tmp" / "architecture-packages"
VERSION = "0.1.0"
REVISION_DATE = "2026-07-14"
SOURCE_COMMIT = "b16cd92436980fb82e01d6e65ff0378d537f5412"
FIXED_ZIP_TIME = (2026, 7, 14, 12, 0, 0)


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def copy_tree(source: Path, target: Path) -> None:
    shutil.copytree(source, target, dirs_exist_ok=True)


def client_readme(package_kind: str) -> str:
    model_note = (
        "This model-exchange package also contains the UAF/SysML XMI, model manifest, "
        "import guidance, and rendering/validation tools. Install their exact environment "
        "with `uv sync --locked`; invoke them with `uv run --locked`. Repository-coverage "
        "checks require the source checkout. Import the XMI only through the process in "
        "`model/MODEL-IMPORT.md`.\n"
        if package_kind == "model-exchange"
        else "The semantic model is distributed in the separate model-exchange package.\n"
    )
    return f"""# Veoveo Reference Architecture {VERSION}

- Package: `{package_kind}`
- Revision: `{REVISION_DATE}`
- Source commit: `{SOURCE_COMMIT}`

Open `index.html` in a browser for the linked offline report. Open
`veoveo-reference-architecture.pdf` for formal review and annotation. Every
diagram is also available as a standalone SVG under `diagrams/`; the complete
component, interface, traceability, and glossary registers are under
`catalogs/`.

{model_note}
This package is a generic reference baseline and contains no client-specific
mission data. Apply the contract's distribution, export-control, CUI, and
classification markings before release. Transfer controlled material only
through the client's approved environment.

Validate file integrity with `SHA256SUMS` before review or import.
"""


def prepare(package_kind: str) -> Path:
    root_name = f"veoveo-reference-architecture-{VERSION}-{package_kind}"
    root = STAGING / root_name
    root.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ARCH / "veoveo-reference-architecture.pdf", root)
    copy_tree(ARCH / "diagrams", root / "diagrams")
    copy_tree(ARCH / "catalogs", root / "catalogs")
    (root / "README.md").write_text(client_readme(package_kind), encoding="utf-8")

    evidence = root / "evidence"
    evidence.mkdir()
    for source, name in [
        (REPO / "README.md", "README.md"),
        (REPO / "docs" / "ARCHITECTURE_DECISIONS.md", "ARCHITECTURE_DECISIONS.md"),
        (REPO / "docs" / "TECH_DESIGN.md", "TECH_DESIGN.md"),
        (REPO / "docs" / "CODEMAP.md", "CODEMAP.md"),
    ]:
        shutil.copy2(source, evidence / name)

    index = (ARCH / "index.html").read_text(encoding="utf-8")
    index = index.replace('href="../../README.md"', 'href="evidence/README.md"')
    index = index.replace(
        'href="../ARCHITECTURE_DECISIONS.md"', 'href="evidence/ARCHITECTURE_DECISIONS.md"'
    )
    index = index.replace('href="../TECH_DESIGN.md"', 'href="evidence/TECH_DESIGN.md"')
    index = index.replace('href="../CODEMAP.md"', 'href="evidence/CODEMAP.md"')
    if package_kind == "review":
        model_links = '<a href="model/veoveo-uaf-sysml.xmi">Veoveo model exchange XMI</a>, <a href="model/model-manifest.yaml">model manifest</a>, and <a href="model/MODEL-IMPORT.md">import guidance</a>.'
        index = index.replace(
            model_links, "The semantic model is distributed in the separate model-exchange package."
        )
    else:
        copy_tree(ARCH / "model", root / "model")
        tools = root / "tools"
        tools.mkdir()
        for name in ("render.py", "validate.py", "package.py"):
            shutil.copy2(ARCH / "tools" / name, tools / name)
        for name in ("pyproject.toml", "uv.lock", ".python-version"):
            shutil.copy2(ARCH / name, root / name)
    (root / "index.html").write_text(index, encoding="utf-8")

    manifest = {
        "schema": "veoveo.architecture-release-manifest/v1",
        "package": package_kind,
        "architecture_id": "VV-MODEL-001",
        "version": VERSION,
        "revision_date": REVISION_DATE,
        "source_commit": SOURCE_COMMIT,
        "handling": "reference_baseline_no_client_data",
        "standards": {"uaf": "1.3", "sysml": "1.6", "uml": "2.5.1", "xmi": "2.5.1"},
        "counts": {
            "software_resources": 50,
            "interfaces": 33,
            "requirements": 18,
            "views": 11,
            "pdf_pages": 24,
        },
    }
    (root / "RELEASE-MANIFEST.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    payload = sorted(
        path for path in root.rglob("*") if path.is_file() and path.name != "SHA256SUMS"
    )
    checksums = "".join(
        f"{digest(path)}  {path.relative_to(root).as_posix()}\n" for path in payload
    )
    (root / "SHA256SUMS").write_text(checksums, encoding="utf-8")
    return root


def deterministic_zip(source_root: Path, destination: Path) -> None:
    with zipfile.ZipFile(
        destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in sorted(source_root.rglob("*")):
            if not path.is_file():
                continue
            arcname = f"{source_root.name}/{path.relative_to(source_root).as_posix()}"
            info = zipfile.ZipInfo(arcname, date_time=FIXED_ZIP_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(
                info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9
            )


def main() -> None:
    subprocess.run([sys.executable, str(ARCH / "tools" / "validate.py")], cwd=REPO, check=True)
    shutil.rmtree(STAGING, ignore_errors=True)
    STAGING.mkdir(parents=True)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for old in OUTPUT.glob("veoveo-reference-architecture-*.zip"):
        old.unlink()
    archives: list[Path] = []
    for kind in ("review", "model-exchange"):
        source = prepare(kind)
        destination = OUTPUT / f"{source.name}.zip"
        deterministic_zip(source, destination)
        archives.append(destination)
    (OUTPUT / "SHA256SUMS").write_text(
        "".join(f"{digest(path)}  {path.name}\n" for path in archives),
        encoding="utf-8",
    )
    pdf_output = REPO / "output" / "pdf"
    pdf_output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(
        ARCH / "veoveo-reference-architecture.pdf", pdf_output / "veoveo-reference-architecture.pdf"
    )
    shutil.rmtree(STAGING)
    for path in archives:
        print(f"created {path.relative_to(REPO)} ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
