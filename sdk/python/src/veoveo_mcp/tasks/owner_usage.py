"""SQL-selected usage tied to the current owner of its linked Task."""

from __future__ import annotations

import uuid
from dataclasses import dataclass

from surrealdb import RecordID

from .queries import OwnerStatement
from ..contract.usage import UsageRecord
from .owner_query import OwnerTaskQuery, native_task_id
from .types import InvalidRecord, TaskError, task_record


@dataclass(frozen=True)
class TaskUsagePage:
    task_ids: tuple[uuid.UUID, ...]
    next_task_id: uuid.UUID | None


@dataclass(frozen=True)
class TaskUsageCompletion:
    task_ids: tuple[uuid.UUID, ...]
    has_more: bool


def _task_id(record: RecordID) -> uuid.UUID:
    if not isinstance(record, RecordID) or record.table_name != "task":
        raise InvalidRecord("usage parent must be a Task record")
    return native_task_id(record.id)


@dataclass(frozen=True)
class OwnerTaskUsageQuery:
    query: OwnerTaskQuery


    async def page(self, after: uuid.UUID | None = None, limit: int = 100) -> TaskUsagePage:
        if type(limit) is not int or not 1 <= limit <= 1000:
            raise TaskError("usage page size must be 1–1000")

        rows = await self.query.runtime.store.query(
            self.query._statement(OwnerStatement.USAGE_PAGE, after=after is not None),
            {**self.query.bindings(), "limit": limit + 1,
             "after": task_record(native_task_id(after)) if after is not None else None},
        )
        records = rows[0] or []
        ids = tuple(_task_id(record) for record in records[:limit])
        return TaskUsagePage(ids, ids[-1] if len(records) > limit else None)

    async def get(self, task_id: uuid.UUID) -> tuple[UsageRecord, ...]:
        task = task_record(native_task_id(task_id))
        rows = await self.query.runtime.store.query(
            self.query._statement(OwnerStatement.USAGE_GET),
            {**self.query.bindings(), "task": task},
        )
        records = []
        for record in rows[0] or []:
            parent = _task_id(record["task"])
            if parent != native_task_id(task_id):
                raise InvalidRecord("selected usage belongs to another Task")
            records.append(UsageRecord(
                taskId=str(parent), modelId=record["model_id"],
                kind=record["kind"], sourceId=record.get("source_id"),
                providerJobId=record.get("provider_job_id"),
                quantity=record.get("quantity"), unit=record.get("unit"),
                amount=record.get("amount"), currency=record.get("currency"),
                recordedAt=record["recorded_at"], metadata=record.get("metadata"),
            ))
        return tuple(records)

    async def complete(self, prefix: str) -> TaskUsageCompletion:
        if len(prefix) > 36:
            return TaskUsageCompletion((), False)
        rows = await self.query.runtime.store.query(
            self.query._statement(OwnerStatement.USAGE_COMPLETE),
            {**self.query.bindings(), "prefix": prefix},
        )
        records = rows[0] or []
        return TaskUsageCompletion(tuple(_task_id(record) for record in records[:100]), len(records) > 100)
