"""Bounded report and usage pages with collection-bound versioned cursors."""
from __future__ import annotations

import base64
import json
from typing import Literal, Self

from pydantic import (
    AwareDatetime, BaseModel, ConfigDict, Field, UUID7, computed_field,
    field_serializer, model_validator,
)
from veoveo_mcp.tasks import TaskPageCursor, TaskStatus

PAGE_SIZE = 100


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate cursor field")
        result[key] = value
    return result


class _Cursor(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    version: Literal[1] = 1

    def encode(self) -> str:
        return base64.urlsafe_b64encode(self.model_dump_json().encode()).decode().rstrip("=")

    @classmethod
    def decode(cls, value: str) -> Self:
        if not value or len(value) > 768:
            raise ValueError("invalid catalog cursor length")
        raw = base64.b64decode(value + "=" * (-len(value) % 4), altchars=b"-_", validate=True)
        if base64.urlsafe_b64encode(raw).decode().rstrip("=") != value:
            raise ValueError("invalid cursor encoding")
        document = json.loads(raw, object_pairs_hook=_unique_object)
        if (not isinstance(document, dict) or type(document.get("version")) is not int
                or not isinstance(document.get("collection"), str)):
            raise ValueError("invalid cursor version")
        return cls.model_validate_json(raw, strict=True)


class ReportCursor(_Cursor):
    collection: Literal["reports"] = "reports"
    created_at: AwareDatetime
    task_id: UUID7

    def position(self) -> TaskPageCursor:
        return TaskPageCursor(self.created_at, self.task_id)


class UsageCursor(_Cursor):
    collection: Literal["usage"] = "usage"
    task_id: UUID7


class UsageEntry(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    task_id: UUID7

    @computed_field
    @property
    def usage_uri(self) -> str:
        from .uris import usage_task_uri

        return usage_task_uri(self.task_id)


class ReportEntry(UsageEntry):
    task_type: Literal["profile_dataset"]
    status: TaskStatus
    created_at: AwareDatetime


class ReportPage(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    items: tuple[ReportEntry, ...] = Field(max_length=PAGE_SIZE)
    limit: Literal[100] = PAGE_SIZE
    next_cursor: ReportCursor | None = None

    @model_validator(mode="after")
    def checked_continuation(self) -> Self:
        if self.next_cursor is not None and (
            len(self.items) != PAGE_SIZE
            or self.next_cursor.task_id != self.items[-1].task_id
            or self.next_cursor.created_at != self.items[-1].created_at
        ):
            raise ValueError("report continuation must follow the last item of a full page")
        return self

    @field_serializer("next_cursor")
    def cursor_wire(self, value: ReportCursor | None) -> str | None:
        return value.encode() if value is not None else None

    @computed_field
    @property
    def next_uri(self) -> str | None:
        from .uris import ReportCatalogResource

        return ReportCatalogResource(self.next_cursor).uri() if self.next_cursor else None


class UsagePage(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    items: tuple[UsageEntry, ...] = Field(max_length=PAGE_SIZE)
    limit: Literal[100] = PAGE_SIZE
    next_cursor: UsageCursor | None = None

    @model_validator(mode="after")
    def checked_continuation(self) -> Self:
        if self.next_cursor is not None and (
            len(self.items) != PAGE_SIZE or self.next_cursor.task_id != self.items[-1].task_id
        ):
            raise ValueError("usage continuation must follow the last item of a full page")
        return self

    @field_serializer("next_cursor")
    def cursor_wire(self, value: UsageCursor | None) -> str | None:
        return value.encode() if value is not None else None

    @computed_field
    @property
    def next_uri(self) -> str | None:
        from .uris import UsageCatalogResource

        return UsageCatalogResource(self.next_cursor).uri() if self.next_cursor else None
