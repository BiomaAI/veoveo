"""Public deployment identity for hosted servers.

Every hosted server mounts at `/{slug}` beneath one public base URL, exactly
as the Rust `PublicDeployment` contract does.
"""

from __future__ import annotations

from dataclasses import dataclass
from urllib.parse import urlsplit


class DeploymentError(ValueError):
    pass


@dataclass(frozen=True)
class ServerPublicEndpoint:
    server_slug: str
    mount_path: str
    public_url: str

    def path(self, child: str) -> str:
        if not child:
            return self.mount_path
        return f"{self.mount_path}/{child}"


@dataclass(frozen=True)
class PublicDeployment:
    base_url: str
    host_authority: str

    @classmethod
    def new(cls, public_base_url: str) -> "PublicDeployment":
        parts = urlsplit(public_base_url.strip())
        if parts.scheme not in ("http", "https") or not parts.netloc:
            raise DeploymentError(
                f"public base URL `{public_base_url}` must be an absolute http(s) URL"
            )
        if parts.path not in ("", "/") or parts.query or parts.fragment:
            raise DeploymentError(
                f"public base URL `{public_base_url}` must not carry a path or query"
            )
        return cls(
            base_url=f"{parts.scheme}://{parts.netloc}", host_authority=parts.netloc
        )

    def server(self, server_slug: str) -> ServerPublicEndpoint:
        slug = server_slug.strip()
        if not slug or any(not (ch.isalnum() or ch in "-_") for ch in slug):
            raise DeploymentError(f"invalid server slug `{server_slug}`")
        mount_path = f"/{slug}"
        return ServerPublicEndpoint(
            server_slug=slug,
            mount_path=mount_path,
            public_url=f"{self.base_url}{mount_path}",
        )


def public_allowed_hosts(
    deployment: PublicDeployment, allow_loopback_hosts: bool
) -> list[str]:
    hosts = [deployment.host_authority]
    if allow_loopback_hosts:
        hosts.extend(["localhost", "127.0.0.1", "::1"])
    return hosts
