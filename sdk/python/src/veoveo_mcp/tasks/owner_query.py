"""Current caller authority composed with Task identity, context and operation SQL."""

from __future__ import annotations

import uuid
from dataclasses import dataclass, replace
from datetime import datetime
from typing import TYPE_CHECKING, Any, Sequence

from surrealdb import RecordID

from .queries import OwnerStatement, owner_statement
from .types import (
    TaskError, TaskInputRequest, TaskInputSubmission, TaskOwner, TaskSnapshot, TaskTypeName,
    deterministic_work_context_id, parse_task_id, profile_record, server_record,
    task_record,
)

if TYPE_CHECKING:
    from .owner_subscriptions import OwnerTaskSubscription
    from .owner_usage import OwnerTaskUsageQuery
    from .runtime import TaskRuntime


def native_task_id(value: uuid.UUID) -> uuid.UUID:
    if not isinstance(value, uuid.UUID):
        raise TaskError("Task queries require a UUID; parse the wire handle on entry")
    return parse_task_id(value)


@dataclass(frozen=True)
class TaskPageCursor:
    created_at: datetime
    task_id: uuid.UUID

    def __post_init__(self) -> None:
        native_task_id(self.task_id)
        if not isinstance(self.created_at, datetime) or self.created_at.utcoffset() is None:
            raise TaskError("Task page cursor requires a timezone-aware timestamp")


@dataclass(frozen=True)
class TaskPage:
    items: tuple[TaskSnapshot, ...]
    next_cursor: TaskPageCursor | None


@dataclass(frozen=True)
class OwnerTaskQuery:
    runtime: TaskRuntime
    owner: TaskOwner
    _task_types: frozenset[TaskTypeName] | None = None
    _context: bool = False

    def in_work_context(self) -> OwnerTaskQuery:
        return replace(self, _context=True)

    def of_type(self, kind: TaskTypeName) -> OwnerTaskQuery:
        return self.of_types((kind,))

    def of_types(self, kinds: Sequence[TaskTypeName]) -> OwnerTaskQuery:
        if not 1 <= len(kinds) <= 32 or any(
            not isinstance(kind, TaskTypeName) for kind in kinds
        ):
            raise TaskError("Task operation selection requires 1–32 typed names")
        return replace(self, _task_types=frozenset(kinds))

    def _statement(self, kind: OwnerStatement, *, after: bool = False) -> str:
        return owner_statement(
            kind, context=self._context, types=self._task_types is not None, after=after,
        )

    def bindings(self) -> dict[str, Any]:
        owner = self.owner
        values = {
            "server": server_record(self.runtime.server),
            "tenant": owner.tenant_record(), "owner": owner.principal_record(),
            "profile": profile_record(owner.profile),
            "principal_key": owner.principal_key, "profile_key": owner.profile,
            "tenant_key": owner.tenant_key, "labels": sorted(owner.data_labels),
        }
        if self._task_types is not None:
            values["task_types"] = sorted(kind.value for kind in self._task_types)
        if self._context:
            values.update({
                "work_context": RecordID("work_context", deterministic_work_context_id(
                    owner.effective_tenant_key(), owner.authority.work_context
                )),
                "work_context_key": owner.authority.work_context,
                "authority_tenant": owner.authority.tenant,
            })
        return values

    async def get(self, task_id: uuid.UUID) -> TaskSnapshot | None:
        from .runtime import _record_to_snapshot

        rows = await self.runtime.store.query(
            self._statement(OwnerStatement.GET),
            {**self.bindings(), "task": task_record(native_task_id(task_id))},
        )
        return _record_to_snapshot(rows[0][0]) if rows[0] else None

    async def page(self, after: TaskPageCursor | None = None, limit: int = 100) -> TaskPage:
        from .runtime import _record_to_snapshot

        if type(limit) is not int or not 1 <= limit <= 1000:
            raise TaskError("Task page size must be 1–1000")

        bindings = {**self.bindings(), "limit": limit + 1}
        if after is not None:
            if not isinstance(after, TaskPageCursor):
                raise TaskError("Task page position must be a TaskPageCursor")

            bindings.update({"after_created_at": after.created_at,
                             "after_task": task_record(after.task_id)})
        rows = await self.runtime.store.query(
            self._statement(OwnerStatement.PAGE, after=after is not None), bindings,
        )
        records = rows[0] or []
        items = tuple(_record_to_snapshot(record) for record in records[:limit])
        cursor = (
            TaskPageCursor(items[-1].created_at, items[-1].task_id)
            if len(records) > limit else None
        )
        return TaskPage(items, cursor)

    async def outstanding_inputs(self, task_id: uuid.UUID) -> dict[str, TaskInputRequest]:
        from .runtime import _input_record_to_exchange

        # The parent selection and child read share one database statement.
        rows = await self.runtime.store.query(
            self._statement(OwnerStatement.INPUTS),
            {**self.bindings(), "task": task_record(native_task_id(task_id))},
        )
        exchanges = [_input_record_to_exchange(record) for record in rows[0] or []]
        return {exchange.key: exchange.request for exchange in exchanges}

    async def subscribe(self, ids: Sequence[uuid.UUID]) -> OwnerTaskSubscription:
        from .owner_subscriptions import subscribe

        return await subscribe(self, ids)

    def usage(self) -> OwnerTaskUsageQuery:
        from .owner_usage import OwnerTaskUsageQuery

        return OwnerTaskUsageQuery(self)

    async def cancel(self, task_id: uuid.UUID) -> TaskSnapshot:
        return await self.runtime.cancel(str(native_task_id(task_id)), owner_query=self)

    async def submit_input_responses(
        self, task_id: uuid.UUID, responses: dict[str, dict[str, Any]]
    ) -> TaskInputSubmission:
        return await self.runtime.submit_input_responses(
            str(native_task_id(task_id)), responses, owner_query=self
        )
