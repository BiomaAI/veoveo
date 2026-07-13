"""Host-authority validation, mirroring the Rust `mcp-contract` host module.

Requests whose Host authority is not on the allowlist are rejected with
421 Misdirected Request before any other processing.
"""

from __future__ import annotations

import ipaddress
from dataclasses import dataclass
from typing import Any, Awaitable, Callable

Scope = dict[str, Any]
Receive = Callable[[], Awaitable[dict[str, Any]]]
Send = Callable[[dict[str, Any]], Awaitable[None]]
AsgiApp = Callable[[Scope, Receive, Send], Awaitable[None]]

_INVALID_AUTHORITY_CHARS = set("/\\?#@")


@dataclass(frozen=True)
class HostAuthority:
    host: str
    port: int | None


def parse_request_host_authority(value: str) -> HostAuthority | None:
    if not value or _contains_invalid_authority_char(value):
        return None
    if value.startswith("["):
        rest = value[1:]
        host, bracket, suffix = rest.partition("]")
        if not bracket or not host:
            return None
        if not suffix:
            return _normalize(host, None)
        if not suffix.startswith(":"):
            return None
        port = _parse_port(suffix[1:])
        if port is None:
            return None
        return _normalize(host, port)

    host, colon, port_text = value.rpartition(":")
    if colon:
        if not host or ":" in host:
            return None
        port = _parse_port(port_text)
        if port is None:
            return None
        return _normalize(host, port)
    return _normalize(value, None)


def parse_allowed_host_authority(value: str) -> HostAuthority | None:
    value = value.strip()
    if not value:
        return None
    authority = parse_request_host_authority(value)
    if authority is not None:
        return authority
    try:
        ipaddress.IPv6Address(value)
    except ValueError:
        return None
    return _normalize(value, None)


def host_authority_is_allowed(
    authority: HostAuthority, allowed_hosts: list[str]
) -> bool:
    for allowed_text in allowed_hosts:
        allowed = parse_allowed_host_authority(allowed_text)
        if allowed is None:
            continue
        if allowed.host != authority.host:
            continue
        if allowed.port is None or authority.port == allowed.port:
            return True
    return False


def _normalize(host: str, port: int | None) -> HostAuthority:
    return HostAuthority(host=host.strip("[]").lower(), port=port)


def _parse_port(value: str) -> int | None:
    if not value or not value.isdigit():
        return None
    port = int(value)
    return port if 0 <= port <= 65_535 else None


def _contains_invalid_authority_char(value: str) -> bool:
    return any(ch.isspace() or ch in _INVALID_AUTHORITY_CHARS for ch in value)


class HostValidationMiddleware:
    def __init__(
        self,
        app: AsgiApp,
        allowed_hosts: list[str],
        logger: Callable[[str], None] | None = None,
    ) -> None:
        self.app = app
        self.allowed_hosts = allowed_hosts
        self.logger = logger

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return
        authority = self._request_authority(scope)
        if authority is None:
            await _respond_status(send, 400)
            return
        if host_authority_is_allowed(authority, self.allowed_hosts):
            await self.app(scope, receive, send)
            return
        if self.logger is not None:
            self.logger(
                f"rejected request for untrusted host {authority.host}:{authority.port}"
            )
        await _respond_status(send, 421)

    @staticmethod
    def _request_authority(scope: Scope) -> HostAuthority | None:
        for name, value in scope.get("headers", []):
            if name.decode("latin-1").lower() == "host":
                return parse_request_host_authority(value.decode("latin-1"))
        server = scope.get("server")
        if server is not None:
            host, port = server
            return parse_request_host_authority(
                f"{host}:{port}" if port is not None else host
            )
        return None


async def _respond_status(send: Send, status: int) -> None:
    await send(
        {
            "type": "http.response.start",
            "status": status,
            "headers": [(b"content-length", b"0")],
        }
    )
    await send({"type": "http.response.body", "body": b""})
