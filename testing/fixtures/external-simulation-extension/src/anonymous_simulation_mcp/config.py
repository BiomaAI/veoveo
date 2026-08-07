"""Fail-closed configuration for the simulator-hosted live-view fixture."""

from __future__ import annotations

import argparse
import ipaddress
import os
from dataclasses import dataclass

from veoveo_mcp.host import parse_allowed_host_authority


SERVER_SLUG = "anonymous-simulation"
DEFAULT_PORT = 8812


@dataclass(frozen=True, slots=True)
class Config:
    port: int
    allowed_hosts: tuple[str, ...]
    internal_trust_jwks: str
    public_signaling_url: str
    public_media_host: str
    public_media_port: int
    lease_seconds: int
    viewer_slots: int


def parse_config(argv: list[str] | None = None) -> Config:
    parser = argparse.ArgumentParser(
        prog="anonymous-simulation-mcp",
        description="Simulator-hosted live-view conformance fixture",
    )
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument(
        "--allowed-host", dest="allowed_hosts", action="append", default=[]
    )
    parser.add_argument(
        "--internal-trust-jwks",
        default=os.environ.get("VEOVEO_INTERNAL_TRUST_JWKS"),
    )
    parser.add_argument(
        "--public-signaling-url",
        default=os.environ.get(
            "ANONYMOUS_SIMULATION_PUBLIC_SIGNALING_URL",
            "ws://127.0.0.1:8812/anonymous-simulation/signaling",
        ),
    )
    parser.add_argument(
        "--public-media-host",
        default=os.environ.get("ANONYMOUS_SIMULATION_PUBLIC_MEDIA_HOST", "127.0.0.1"),
    )
    parser.add_argument(
        "--public-media-port",
        type=int,
        default=int(os.environ.get("ANONYMOUS_SIMULATION_PUBLIC_MEDIA_PORT", "48030")),
    )
    parser.add_argument(
        "--lease-seconds",
        type=int,
        default=int(os.environ.get("ANONYMOUS_SIMULATION_LEASE_SECONDS", "120")),
    )
    parser.add_argument(
        "--viewer-slots",
        type=int,
        default=int(os.environ.get("ANONYMOUS_SIMULATION_VIEWER_SLOTS", "2")),
    )
    args = parser.parse_args(argv)

    if not 1_024 <= args.port <= 65_535:
        parser.error("--port must be between 1024 and 65535")
    if not args.allowed_hosts:
        parser.error("at least one --allowed-host is required")
    for host in args.allowed_hosts:
        if parse_allowed_host_authority(host) is None:
            parser.error(f"invalid --allowed-host {host!r}")
    if not args.internal_trust_jwks:
        parser.error("--internal-trust-jwks is required")
    if not args.public_signaling_url.startswith(("ws://", "wss://")):
        parser.error("--public-signaling-url must be an absolute WS(S) URL")
    if "@" in args.public_signaling_url:
        parser.error("--public-signaling-url must not contain credentials")
    try:
        ipaddress.ip_address(args.public_media_host)
    except ValueError:
        parser.error("--public-media-host must be a numeric IP address")
    if not 1_024 <= args.public_media_port <= 65_535:
        parser.error("--public-media-port must be between 1024 and 65535")
    if not 5 <= args.lease_seconds <= 3_600:
        parser.error("--lease-seconds must be between 5 and 3600")
    if not 1 <= args.viewer_slots <= 32:
        parser.error("--viewer-slots must be between 1 and 32")
    if args.public_media_port + args.viewer_slots - 1 > 65_535:
        parser.error("the viewer-slot media port range exceeds 65535")
    return Config(
        port=args.port,
        allowed_hosts=tuple(args.allowed_hosts),
        internal_trust_jwks=args.internal_trust_jwks,
        public_signaling_url=args.public_signaling_url,
        public_media_host=args.public_media_host,
        public_media_port=args.public_media_port,
        lease_seconds=args.lease_seconds,
        viewer_slots=args.viewer_slots,
    )
