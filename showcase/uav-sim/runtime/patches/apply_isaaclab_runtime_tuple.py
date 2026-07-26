#!/usr/bin/env python3
"""Apply Veoveo's exact Warp/Newton tuple to one pinned Isaac Lab source tree."""

from __future__ import annotations

import argparse
from pathlib import Path

REPLACEMENTS = {
    "source/isaaclab/setup.py": {
        '"warp-lang==1.13.0"': ('"warp-lang==1.15.0"', 1),
    },
    "source/isaaclab_newton/setup.py": {
        '"newton[sim]==1.2.1"': ('"newton[sim]==1.4.0"', 1),
    },
    "source/isaaclab_physx/setup.py": {
        '"newton[sim]==1.2.1"': ('"newton[sim]==1.4.0"', 1),
    },
    "source/isaaclab_visualizers/setup.py": {
        '"newton[sim]==1.2.1"': ('"newton[sim]==1.4.0"', 3),
    },
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    for relative, replacements in REPLACEMENTS.items():
        path = args.source / relative
        text = path.read_text(encoding="utf-8")
        for old, (new, expected_count) in replacements.items():
            actual_count = text.count(old)
            if actual_count != expected_count:
                raise RuntimeError(
                    f"{relative} contains {actual_count} occurrences of {old}; "
                    f"expected {expected_count}"
                )
            text = text.replace(old, new)
        path.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
