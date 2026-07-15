#!/usr/bin/env python3
"""Render architecture PDFs into deterministic visual-review sheets."""

from __future__ import annotations

import argparse
import shutil
from dataclasses import dataclass
from pathlib import Path

import pypdfium2 as pdfium
from PIL import Image, ImageOps
from pypdf import PdfReader

REPO = Path(__file__).resolve().parents[3]
DEFAULT_PDF = REPO / "docs" / "architecture" / "veoveo-reference-architecture.pdf"
OUTPUT = REPO / "tmp" / "architecture-visual-qa"


@dataclass(frozen=True)
class ReviewTarget:
    name: str
    pdf: Path


def render_pages(target: ReviewTarget, output: Path, scale: float) -> list[Path]:
    destination = output / target.name / "pages"
    destination.mkdir(parents=True, exist_ok=True)
    document = pdfium.PdfDocument(target.pdf)
    rendered: list[Path] = []
    try:
        for index in range(len(document)):
            page = document[index]
            bitmap = page.render(scale=scale)
            image = bitmap.to_pil().convert("RGB")
            path = destination / f"page-{index + 1:02d}.png"
            image.save(path, format="PNG", optimize=True)
            rendered.append(path)
            image.close()
            bitmap.close()
            page.close()
    finally:
        document.close()
    return rendered


def contact_sheet(
    pages: list[Path],
    destination: Path,
    *,
    columns: int = 5,
    thumb_size: tuple[int, int] = (420, 300),
    gutter: int = 12,
) -> None:
    rows = (len(pages) + columns - 1) // columns
    width = columns * thumb_size[0] + (columns + 1) * gutter
    height = rows * thumb_size[1] + (rows + 1) * gutter
    sheet = Image.new("RGB", (width, height), "#d7d7d7")
    try:
        for index, path in enumerate(pages):
            with Image.open(path) as source:
                thumbnail = ImageOps.contain(source.convert("RGB"), thumb_size)
            x = gutter + (index % columns) * (thumb_size[0] + gutter)
            y = gutter + (index // columns) * (thumb_size[1] + gutter)
            x += (thumb_size[0] - thumbnail.width) // 2
            y += (thumb_size[1] - thumbnail.height) // 2
            sheet.paste(thumbnail, (x, y))
            thumbnail.close()
        destination.parent.mkdir(parents=True, exist_ok=True)
        sheet.save(destination, format="PNG", optimize=True)
    finally:
        sheet.close()


def verify_pdf(target: ReviewTarget) -> int:
    if not target.pdf.is_file() or not target.pdf.read_bytes().startswith(b"%PDF"):
        raise RuntimeError(f"missing or invalid PDF: {target.pdf}")
    reader = PdfReader(target.pdf)
    pages = len(reader.pages)
    if pages < 20:
        raise RuntimeError(f"unexpected page count for {target.name}: {pages}")
    return pages


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--pdf",
        type=Path,
        default=DEFAULT_PDF,
        help="PDF to render",
    )
    parser.add_argument(
        "--name",
        default="reference",
        help="filesystem-safe output label",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=OUTPUT,
        help="visual-QA output directory",
    )
    parser.add_argument(
        "--scale",
        type=float,
        default=1.5,
        help="PDFium render scale",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="remove prior visual-QA output before rendering",
    )
    args = parser.parse_args()
    if args.scale <= 0:
        parser.error("--scale must be positive")
    if not args.name or any(
        character not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for character in args.name
    ):
        parser.error("--name must contain only lowercase letters, digits, hyphens, or underscores")
    pdf = args.pdf if args.pdf.is_absolute() else REPO / args.pdf
    output = args.output if args.output.is_absolute() else REPO / args.output
    output = output.resolve()
    if output == REPO or REPO not in output.parents:
        parser.error("--output must be a directory inside the repository")
    if args.clean:
        shutil.rmtree(output, ignore_errors=True)

    target = ReviewTarget(args.name, pdf.resolve())
    expected_pages = verify_pdf(target)
    pages = render_pages(target, output, args.scale)
    if len(pages) != expected_pages:
        raise RuntimeError(f"rendered {len(pages)} of {expected_pages} pages for {target.name}")
    sheet = output / target.name / "contact-sheet.png"
    contact_sheet(pages, sheet)
    print(
        f"rendered {target.name}: {expected_pages} pages, contact sheet {sheet.relative_to(REPO)}"
    )


if __name__ == "__main__":
    main()
