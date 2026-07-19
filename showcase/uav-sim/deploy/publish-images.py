#!/usr/bin/env python3
"""Publish commit-addressed UAV images while reusing immutable base layers."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
BASE_VERSION = "isaac-6.0.1-cesium-0.29.0-pegasus-5.1.0-px4-1.17.0"
REGISTRY_PATTERN = re.compile(
    r"^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?(?::[0-9]{1,5})?(?:/[a-z0-9._-]+)*$"
)


def run(*arguments: str, capture: bool = False) -> str:
    completed = subprocess.run(
        arguments,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return completed.stdout.strip() if capture else ""


def source_revision() -> str:
    revision = run("git", "rev-parse", "--verify", "HEAD", capture=True)
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise SystemExit(f"git returned a non-canonical revision: {revision!r}")
    if run("git", "status", "--porcelain", capture=True):
        raise SystemExit(
            "refusing to publish a commit-addressed image from a dirty worktree; "
            "commit the runtime concern first"
        )
    return revision


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Push the immutable UAV base and commit-addressed overlays"
    )
    parser.add_argument(
        "--registry",
        default="veoveo-registry.localhost:5001",
        help="OCI registry host, port, and optional repository prefix",
    )
    parser.add_argument(
        "--skip-base",
        action="store_true",
        help="publish only commit-addressed runtime and MCP targets",
    )
    args = parser.parse_args()
    registry = args.registry.rstrip("/")
    if not REGISTRY_PATTERN.fullmatch(registry):
        raise SystemExit(f"invalid OCI registry path: {registry!r}")

    revision = source_revision()
    base_ref = f"{registry}/veoveo/uav-sim-base:{BASE_VERSION}"
    runtime_ref = f"{registry}/veoveo/uav-sim-runtime:{revision}"
    mcp_ref = f"{registry}/veoveo/uav-sim-mcp:{revision}"
    forwarder_ref = f"{registry}/veoveo/recording-forwarder:{revision}"
    if not args.skip_base:
        run(
            "docker",
            "buildx",
            "bake",
            "uav-sim-base",
            "--set",
            f"uav-sim-base.tags={base_ref}",
            "--push",
        )
    run(
        "docker",
        "buildx",
        "bake",
        "uav-sim-runtime",
        "uav-sim-mcp",
        "recording-forwarder",
        "--set",
        f"uav-sim-runtime.tags={runtime_ref}",
        "--set",
        f"uav-sim-runtime.args.UAV_SIM_BASE_IMAGE={base_ref}",
        "--set",
        f"uav-sim-runtime.args.SOURCE_REVISION={revision}",
        "--set",
        f"uav-sim-mcp.tags={mcp_ref}",
        "--set",
        f"recording-forwarder.tags={forwarder_ref}",
        "--push",
    )
    print(f"UAV base:    {base_ref}")
    print(f"UAV runtime: {runtime_ref}")
    print(f"UAV MCP:     {mcp_ref}")
    print(f"Forwarder:   {forwarder_ref}")


if __name__ == "__main__":
    main()
