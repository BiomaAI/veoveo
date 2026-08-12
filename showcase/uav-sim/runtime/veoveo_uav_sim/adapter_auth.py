from __future__ import annotations

import hmac
from collections.abc import Awaitable, Callable

from aiohttp import web


def authorization_middleware(
    bearer_token: str,
) -> Callable[
    [web.Request, Callable[[web.Request], Awaitable[web.StreamResponse]]],
    Awaitable[web.StreamResponse],
]:
    expected = f"Bearer {bearer_token}"

    @web.middleware
    async def authorize(
        request: web.Request,
        handler: Callable[[web.Request], Awaitable[web.StreamResponse]],
    ) -> web.StreamResponse:
        if request.path in {"/healthz", "/readyz"}:
            return await handler(request)
        supplied = request.headers.get("Authorization", "")
        if not hmac.compare_digest(supplied, expected):
            return web.json_response(
                {"error": "simulator adapter authorization failed"}, status=401
            )
        return await handler(request)

    return authorize
