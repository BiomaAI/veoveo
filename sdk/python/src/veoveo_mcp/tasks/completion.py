"""MCP C02 admission before the Task runtime stores opaque completion results."""

from typing import Any

from mcp.types import CallToolResult

from ..types import ResourceUri
from .types import InvalidRecord, TaskTransition


def mcp_task_completion(message: str, result: dict[str, Any] | CallToolResult) -> TaskTransition:
    """Check product/link agreement independently of the MCP isError flag."""
    try:
        model = result if isinstance(result, CallToolResult) else CallToolResult.model_validate(result)
        payload = model.model_dump(mode="json", by_alias=True, exclude_none=True)
        # Inspect admitted structured content directly: an explicit null address
        # must never become an omitted no-product declaration during dumping.
        structured = model.structured_content
        links = [item for item in payload["content"] if item["type"] == "resource_link"]
        if isinstance(structured, dict) and "result_uri" in structured:
            raise ValueError("obsolete product address field")
        declared = isinstance(structured, dict) and "resultUri" in structured
        if declared:
            raw = structured["resultUri"]
            if not isinstance(raw, str):
                raise ValueError("invalid product address")
            uri = ResourceUri(raw)
            if len(links) != 1 or links[0]["uri"] != str(uri):
                raise ValueError("product link disagreement")
        else:
            uri = None
            if links:
                raise ValueError("product link has no declared address")
    except (ValueError, TypeError, KeyError) as error:
        raise InvalidRecord("Task completion product address disagrees with its MCP result") from error
    return TaskTransition.succeeded(message, payload, result_uri=uri)
