"""Canonical `datasheet://` resource identities."""

from __future__ import annotations

SCHEME = "datasheet"
REPORTS_URI = "datasheet://reports"
USAGE_ROOT_URI = "datasheet://usage"
USAGE_TASK_TEMPLATE = "datasheet://usage/task/{task_id}"
ARTIFACT_TEMPLATE = "datasheet://artifact/{artifact_id}"

_USAGE_TASK_PREFIX = "datasheet://usage/task/"
_ARTIFACT_PREFIX = "datasheet://artifact/"


def usage_task_uri(task_id: str) -> str:
    return f"{_USAGE_TASK_PREFIX}{task_id}"


def artifact_uri(artifact_id: str) -> str:
    return f"{_ARTIFACT_PREFIX}{artifact_id}"


def parse_usage_task_uri(uri: str) -> str | None:
    if uri.startswith(_USAGE_TASK_PREFIX):
        task_id = uri[len(_USAGE_TASK_PREFIX) :]
        return task_id or None
    return None


def parse_artifact_uri(uri: str) -> str | None:
    if uri.startswith(_ARTIFACT_PREFIX):
        artifact_id = uri[len(_ARTIFACT_PREFIX) :]
        return artifact_id or None
    return None
