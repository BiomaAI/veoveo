"""Fail-closed configuration for the anonymous external fixture."""

from __future__ import annotations

import argparse
import os
from dataclasses import dataclass
from pathlib import Path

from veoveo_mcp.host import parse_allowed_host_authority
from veoveo_mcp.simulation_pose import PoseTlsConfig


SERVER_SLUG = "anonymous-simulation"
DEFAULT_PORT = 8812


@dataclass(frozen=True, slots=True)
class Config:
    port: int
    allowed_hosts: tuple[str, ...]
    internal_trust_jwks: str
    artifact_service_url: str
    producer_id: str
    producer_spiffe_id: str
    pose_tls: PoseTlsConfig


def parse_config(argv: list[str] | None = None) -> Config:
    parser = argparse.ArgumentParser(
        prog="anonymous-simulation-mcp",
        description="Anonymous external Simulation View producer fixture",
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
        "--artifact-service-url",
        default=os.environ.get(
            "ARTIFACT_SERVICE_URL", "http://artifact-service:8790"
        ),
    )
    parser.add_argument(
        "--producer-id",
        default=os.environ.get(
            "ANONYMOUS_SIMULATION_PRODUCER_ID", "anonymous-synthetic"
        ),
    )
    parser.add_argument(
        "--producer-spiffe-id",
        default=os.environ.get("ANONYMOUS_SIMULATION_PRODUCER_SPIFFE_ID"),
    )
    parser.add_argument(
        "--pose-host",
        default=os.environ.get(
            "ANONYMOUS_SIMULATION_POSE_HOST", "simulation-view-pose"
        ),
    )
    parser.add_argument(
        "--pose-port",
        type=int,
        default=int(os.environ.get("ANONYMOUS_SIMULATION_POSE_PORT", "7443")),
    )
    parser.add_argument(
        "--pose-server-hostname",
        default=os.environ.get("ANONYMOUS_SIMULATION_POSE_SERVER_HOSTNAME"),
    )
    parser.add_argument(
        "--pose-ca-certificate",
        type=Path,
        default=Path(
            os.environ.get(
                "ANONYMOUS_SIMULATION_POSE_CA_CERTIFICATE",
                "/run/secrets/simulation-view-pose/ca.crt",
            )
        ),
    )
    parser.add_argument(
        "--pose-client-certificate",
        type=Path,
        default=Path(
            os.environ.get(
                "ANONYMOUS_SIMULATION_POSE_CLIENT_CERTIFICATE",
                "/run/secrets/simulation-view-pose/tls.crt",
            )
        ),
    )
    parser.add_argument(
        "--pose-client-private-key",
        type=Path,
        default=Path(
            os.environ.get(
                "ANONYMOUS_SIMULATION_POSE_CLIENT_PRIVATE_KEY",
                "/run/secrets/simulation-view-pose/tls.key",
            )
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
    if not args.artifact_service_url.startswith(("http://", "https://")):
        parser.error("--artifact-service-url must be an absolute HTTP(S) URL")
    if (
        not args.producer_id
        or len(args.producer_id) > 128
        or not args.producer_spiffe_id
        or not args.producer_spiffe_id.startswith("spiffe://")
        or any(character.isspace() for character in args.producer_spiffe_id)
    ):
        parser.error("producer identity and SPIFFE URI are required and bounded")
    if not args.pose_server_hostname:
        parser.error("--pose-server-hostname is required")

    try:
        pose_tls = PoseTlsConfig(
            host=args.pose_host,
            port=args.pose_port,
            server_hostname=args.pose_server_hostname,
            ca_certificate=args.pose_ca_certificate,
            client_certificate=args.pose_client_certificate,
            client_private_key=args.pose_client_private_key,
        )
    except ValueError as error:
        parser.error(str(error))
    return Config(
        port=args.port,
        allowed_hosts=tuple(args.allowed_hosts),
        internal_trust_jwks=args.internal_trust_jwks,
        artifact_service_url=args.artifact_service_url.rstrip("/"),
        producer_id=args.producer_id,
        producer_spiffe_id=args.producer_spiffe_id,
        pose_tls=pose_tls,
    )
