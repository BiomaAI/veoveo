"""Canonical `datasheet://` resource identities."""

from __future__ import annotations

SCHEME = "datasheet"
REPORTS_URI = "datasheet://reports"
WORKBENCH_APP_URI = "ui://datasheet/workbench.html"
USAGE_ROOT_URI = "datasheet://usage"
USAGE_TASK_TEMPLATE = "datasheet://usage/task/{task_id}"
ARTIFACT_TEMPLATE = "datasheet://artifact/{artifact_id}"
DOCS_URI = "datasheet://docs"
CONTRACT_URI = "datasheet://contract"

_USAGE_TASK_PREFIX = "datasheet://usage/task/"
_ARTIFACT_PREFIX = "datasheet://artifact/"
_DOC_PREFIX = "datasheet://docs/"


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


def doc_uri(doc_id: str) -> str:
    return f"{_DOC_PREFIX}{doc_id}"


def parse_doc_uri(uri: str) -> str | None:
    if uri.startswith(_DOC_PREFIX):
        doc_id = uri[len(_DOC_PREFIX) :]
        return doc_id or None
    return None
