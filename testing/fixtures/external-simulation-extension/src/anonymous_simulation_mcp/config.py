"""Fail-closed configuration for the simulator-hosted live-view fixture."""

from __future__ import annotations

import argparse
import os
from dataclasses import dataclass
from urllib.parse import urlsplit

from veoveo_mcp.host import parse_allowed_host_authority


SERVER_SLUG = "anonymous-simulation"
DEFAULT_PORT = 8812


@dataclass(frozen=True, slots=True)
class Config:
    port: int
    allowed_hosts: tuple[str, ...]
    internal_trust_jwks: str
    public_stream_url: str
    authorization_seconds: int


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
        "--public-stream-url",
        default=os.environ.get(
            "ANONYMOUS_SIMULATION_PUBLIC_STREAM_URL",
            "ws://127.0.0.1:8812/anonymous-simulation/live",
        ),
    )
    parser.add_argument(
        "--authorization-seconds",
        type=int,
        default=int(
            os.environ.get("ANONYMOUS_SIMULATION_AUTHORIZATION_SECONDS", "3600")
        ),
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
    stream_url = urlsplit(args.public_stream_url)
    loopback = stream_url.hostname in {"127.0.0.1", "::1", "localhost"}
    if (
        stream_url.scheme not in ({"ws", "wss"} if loopback else {"wss"})
        or stream_url.hostname is None
        or stream_url.username is not None
        or stream_url.password is not None
        or stream_url.query
        or stream_url.fragment
    ):
        parser.error("--public-stream-url must be a credential-free secure WS URL")
    if not 5 <= args.authorization_seconds <= 86_400:
        parser.error("--authorization-seconds must be between 5 and 86400")
    return Config(
        port=args.port,
        allowed_hosts=tuple(args.allowed_hosts),
        internal_trust_jwks=args.internal_trust_jwks,
        public_stream_url=args.public_stream_url,
        authorization_seconds=args.authorization_seconds,
    )
