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
RTX_NVRTC_ROOT = ISAAC_ROOT / "exts/isaacsim.pip.nv/pip_prebundle/nvidia/cuda_nvrtc/lib"
WARP_ROOT = ISAAC_ROOT / "extscache/omni.warp.core-1.16.0+lx64/warp"
NEWTON_ROOT = ISAAC_ROOT / "exts/isaacsim.pip.newton/pip_prebundle/newton"
EXPECTED_ISAAC_LAB_PACKAGES = {
    "isaaclab": "17.0.2",
    "isaaclab_newton": "5.4.1",
    "isaaclab_ov": "2.2.0",
    "isaaclab_physx": "6.0.0",
}
SYNTHETIC_TORCH_MODULES = {
    ("torch.classes", "_classes.py"),
    ("torch.ops", "_ops.py"),
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
    config = ISAAC_LAB_ROOT / "source" / package / "pyproject.toml"
    with config.open("rb") as stream:
        return str(tomllib.load(stream)["project"]["version"])


def inspect_identity() -> dict[str, object]:
    import newton
    import torch
    import warp

    if warp.__version__ != "1.16.0":
        raise RuntimeError(f"expected Warp 1.16.0, loaded {warp.__version__}")
    if newton.__version__ != "1.5.2":
        raise RuntimeError(f"expected Newton 1.5.2, loaded {newton.__version__}")
    if torch.__version__ != "2.11.0+cu128":
        raise RuntimeError(f"expected Torch 2.11.0+cu128, loaded {torch.__version__}")
    if torch.version.cuda != "12.8":
        raise RuntimeError(f"expected Torch CUDA 12.8, loaded {torch.version.cuda}")
    if not _under(_module_path(torch), TORCH_ROOT):
        raise RuntimeError(f"Torch resolved outside {TORCH_ROOT}: {_module_path(torch)}")
    if not _under(_module_path(warp), WARP_ROOT):
        raise RuntimeError(f"Warp resolved outside {WARP_ROOT}: {_module_path(warp)}")
    if not _under(_module_path(newton), NEWTON_ROOT):
        raise RuntimeError(f"Newton resolved outside {NEWTON_ROOT}: {_module_path(newton)}")
    for package in ("functorch", "torchgen"):
        if not (ML_PREBUNDLE_ROOT / package).is_dir():
            raise RuntimeError(
                f"Torch support package {package} is missing from {ML_PREBUNDLE_ROOT}"
            )
    if not (ISAAC_ROOT / "exts/isaacsim.pip.nv/pip_prebundle/nvidia").is_dir():
        raise RuntimeError("Isaac CUDA package root is missing")
    for library in (
        "libnvrtc-builtins.so.12.8",
        "libnvrtc-builtins.alt.so.12.8",
    ):
        if not (RTX_NVRTC_ROOT / library).exists():
            raise RuntimeError(
                f"Isaac RTX NVRTC builtin {library} is missing from {RTX_NVRTC_ROOT}"
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
        "nvidia": ISAAC_ROOT / "exts/isaacsim.pip.nv/pip_prebundle/nvidia",
        "torch": TORCH_ROOT,
        "torchgen": ML_PREBUNDLE_ROOT / "torchgen",
    }
    for name, module in sorted(sys.modules.items()):
        family = name.partition(".")[0]
        root = approved_roots.get(family)
        if root is None:
            continue
        module_file = getattr(module, "__file__", None)
        if not module_file:
            continue
        if (name, module_file) in SYNTHETIC_TORCH_MODULES:
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
