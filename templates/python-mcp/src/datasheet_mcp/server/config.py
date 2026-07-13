"""Validated CLI/environment configuration for the datasheet server."""

from __future__ import annotations

import argparse
import os
from dataclasses import dataclass

from veoveo_mcp.deployment import PublicDeployment
from veoveo_mcp.host import parse_allowed_host_authority

SERVER_SLUG = "datasheet"
DEFAULT_PORT = 8798


@dataclass
class Config:
    port: int
    public_base_url: str
    artifact_service_url: str
    max_artifact_bytes: int
    max_dataset_bytes: int
    allow_loopback_hosts: bool
    allowed_hosts: list[str]
    surreal_endpoint: str
    surreal_namespace: str
    surreal_database: str
    surreal_username: str
    surreal_password: str
    internal_trust_jwks: str

    def public_deployment(self) -> PublicDeployment:
        return PublicDeployment.new(self.public_base_url)


def parse_config(argv: list[str] | None = None) -> Config:
    parser = argparse.ArgumentParser(
        prog="datasheet-mcp", description="Datasheet MCP server (streamable HTTP)"
    )
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument(
        "--public-base-url", default=os.environ.get("PUBLIC_BASE_URL")
    )
    parser.add_argument(
        "--artifact-service-url", default="http://artifact-service:8790"
    )
    parser.add_argument("--max-artifact-bytes", type=int, default=67_108_864)
    parser.add_argument(
        "--max-dataset-bytes",
        type=int,
        default=16_777_216,
        help="Largest dataset materialized into a durable task request.",
    )
    parser.add_argument("--allow-loopback-hosts", action="store_true")
    parser.add_argument(
        "--allowed-host", dest="allowed_hosts", action="append", default=[]
    )
    parser.add_argument(
        "--surreal-endpoint", default=os.environ.get("VEOVEO_SURREAL_ENDPOINT")
    )
    parser.add_argument(
        "--surreal-namespace", default=os.environ.get("VEOVEO_SURREAL_NAMESPACE")
    )
    parser.add_argument(
        "--surreal-database", default=os.environ.get("VEOVEO_SURREAL_DATABASE")
    )
    parser.add_argument(
        "--surreal-auth-level", default=os.environ.get("VEOVEO_SURREAL_AUTH_LEVEL")
    )
    parser.add_argument(
        "--surreal-username", default=os.environ.get("VEOVEO_SURREAL_USERNAME")
    )
    parser.add_argument(
        "--surreal-password", default=os.environ.get("VEOVEO_SURREAL_PASSWORD")
    )
    parser.add_argument(
        "--internal-trust-jwks", default=os.environ.get("VEOVEO_INTERNAL_TRUST_JWKS")
    )
    args = parser.parse_args(argv)

    def require(name: str, value: str | None) -> str:
        if not value:
            parser.error(f"{name} is required")
        return value

    if require("surreal auth level", args.surreal_auth_level) != "database":
        parser.error("datasheet requires database-scoped SurrealDB credentials")
    for host in args.allowed_hosts:
        if parse_allowed_host_authority(host) is None:
            parser.error(
                f"--allowed-host `{host}` must be a host authority such as "
                "datasheet-mcp:8798"
            )
    return Config(
        port=args.port,
        public_base_url=require("--public-base-url", args.public_base_url),
        artifact_service_url=args.artifact_service_url,
        max_artifact_bytes=args.max_artifact_bytes,
        max_dataset_bytes=args.max_dataset_bytes,
        allow_loopback_hosts=args.allow_loopback_hosts,
        allowed_hosts=list(args.allowed_hosts),
        surreal_endpoint=require("--surreal-endpoint", args.surreal_endpoint),
        surreal_namespace=require("--surreal-namespace", args.surreal_namespace),
        surreal_database=require("--surreal-database", args.surreal_database),
        surreal_username=require("--surreal-username", args.surreal_username),
        surreal_password=require("--surreal-password", args.surreal_password),
        internal_trust_jwks=require(
            "--internal-trust-jwks", args.internal_trust_jwks
        ),
    )
