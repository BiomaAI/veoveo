#!/usr/bin/env python3
"""Verify the canonical simulation runtime's module and source identities."""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path
from types import ModuleType


ISAAC_ROOT = Path("/isaac-sim")
ISAAC_LAB_ROOT = Path("/opt/veoveo/isaaclab")
ML_PREBUNDLE_ROOT = (
    ISAAC_ROOT / "extsDeprecated/omni.isaac.ml_archive/pip_prebundle"
)
TORCH_ROOT = ML_PREBUNDLE_ROOT / "torch"
WARP_ROOT = ISAAC_ROOT / "extscache/omni.warp.core-1.13.0+lx64/warp"
NEWTON_ROOT = ISAAC_ROOT / "exts/isaacsim.pip.newton/pip_prebundle/newton"
EXPECTED_ISAAC_LAB_PACKAGES = {
    "isaaclab": "6.1.14",
    "isaaclab_newton": "0.13.6",
    "isaaclab_ov": "0.4.2",
    "isaaclab_ovphysx": "3.0.2",
    "isaaclab_physx": "1.1.3",
}


def _under(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError:
        return False
    return True


def _module_path(module: ModuleType) -> Path:
    value = getattr(module, "__file__", None)
    if not value:
        raise RuntimeError(f"module {module.__name__!r} has no filesystem identity")
    return Path(value).resolve()


def _isaac_lab_version(package: str) -> str:
    config = ISAAC_LAB_ROOT / "source" / package / "config/extension.toml"
    with config.open("rb") as stream:
        return str(tomllib.load(stream)["package"]["version"])


def inspect_identity() -> dict[str, object]:
    import newton
    import torch
    import warp

    if warp.__version__ != "1.15.0":
        raise RuntimeError(f"expected Warp 1.15.0, loaded {warp.__version__}")
    if newton.__version__ != "1.4.0":
        raise RuntimeError(f"expected Newton 1.4.0, loaded {newton.__version__}")
    if torch.__version__ != "2.12.0+cu130":
        raise RuntimeError(f"expected Torch 2.12.0+cu130, loaded {torch.__version__}")
    if torch.version.cuda != "13.0":
        raise RuntimeError(f"expected Torch CUDA 13.0, loaded {torch.version.cuda}")
    if not _under(_module_path(torch), TORCH_ROOT):
        raise RuntimeError(f"Torch resolved outside {TORCH_ROOT}: {_module_path(torch)}")
    if not _under(_module_path(warp), WARP_ROOT):
        raise RuntimeError(f"Warp resolved outside {WARP_ROOT}: {_module_path(warp)}")
    if not _under(_module_path(newton), NEWTON_ROOT):
        raise RuntimeError(f"Newton resolved outside {NEWTON_ROOT}: {_module_path(newton)}")
    for package in ("functorch", "nvidia", "torchgen", "triton"):
        if not (ML_PREBUNDLE_ROOT / package).is_dir():
            raise RuntimeError(
                f"Torch support package {package} is missing from {ML_PREBUNDLE_ROOT}"
            )
    for unsupported in ("torchaudio", "torchvision", "torchvision.libs"):
        if (ML_PREBUNDLE_ROOT / unsupported).exists():
            raise RuntimeError(
                f"unsupported bundled package remains in Torch root: {unsupported}"
            )

    isaac_lab: dict[str, dict[str, str]] = {}
    for package, expected_version in EXPECTED_ISAAC_LAB_PACKAGES.items():
        module = __import__(package)
        actual_version = _isaac_lab_version(package)
        if actual_version != expected_version:
            raise RuntimeError(
                f"expected {package} {expected_version}, found {actual_version}"
            )
        module_path = _module_path(module)
        source_root = ISAAC_LAB_ROOT / "source" / package
        if not _under(module_path, source_root):
            raise RuntimeError(
                f"{package} resolved outside immutable source root: {module_path}"
            )
        isaac_lab[package] = {
            "version": actual_version,
            "file": str(module_path),
        }

    mixed_modules: list[dict[str, str]] = []
    inspected_modules = 0
    approved_roots = {
        "functorch": ML_PREBUNDLE_ROOT / "functorch",
        "warp": WARP_ROOT,
        "newton": NEWTON_ROOT,
        "nvidia": ML_PREBUNDLE_ROOT / "nvidia",
        "torch": TORCH_ROOT,
        "torchgen": ML_PREBUNDLE_ROOT / "torchgen",
        "triton": ML_PREBUNDLE_ROOT / "triton",
    }
    for name, module in sorted(sys.modules.items()):
        family = name.partition(".")[0]
        root = approved_roots.get(family)
        if root is None:
            continue
        module_file = getattr(module, "__file__", None)
        if not module_file:
            continue
        inspected_modules += 1
        if not _under(Path(module_file), root):
            mixed_modules.append({"module": name, "file": str(module_file)})
    if mixed_modules:
        raise RuntimeError(
            "mixed authoritative module roots detected: "
            + json.dumps(mixed_modules, sort_keys=True)
        )

    return {
        "isaac_sim": (ISAAC_ROOT / "VERSION").read_text().strip(),
        "python": f"{sys.version_info.major}.{sys.version_info.minor}",
        "torch": {
            "version": torch.__version__,
            "cuda": torch.version.cuda,
            "file": str(_module_path(torch)),
        },
        "warp": {
            "version": warp.__version__,
            "file": str(_module_path(warp)),
        },
        "newton": {
            "version": newton.__version__,
            "file": str(_module_path(newton)),
        },
        "isaac_lab": isaac_lab,
        "inspected_authoritative_modules": inspected_modules,
        "mixed_module_roots": mixed_modules,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        help="optional path for the canonical JSON result",
    )
    args = parser.parse_args()
    result = inspect_identity()
    encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output is not None:
        args.output.write_text(encoded)
    print(
        "SIMULATION_RUNTIME_IDENTITY=" + json.dumps(result, sort_keys=True),
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
