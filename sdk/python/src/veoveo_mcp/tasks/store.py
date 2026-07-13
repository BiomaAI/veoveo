"""SurrealDB platform-store access for Python MCP servers.

A focused port of the `veoveo-platform-store` surfaces the task runtime and
domain servers need: checked multi-statement queries, canonical identity
upserts, the transactional outbox, and domain usage. Schema migrations remain
owned by the Rust `platform-store` crate; this module only reads and writes
the existing schema with the database-level runtime user.
"""

from __future__ import annotations

import asyncio
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, AsyncIterator

from surrealdb import AsyncSurreal, RecordID

from .types import (
    InvalidRecord,
    PrincipalKind,
    TaskOwner,
    deterministic_enterprise_id,
    deterministic_principal_id,
    deterministic_tenant_id,
    parse_task_id,
    server_record,
)

MAX_OUTBOX_LIMIT = 1_000
MAX_TRANSACTION_ATTEMPTS = 8
DOMAIN_USAGE_EVENT_SCHEMA_VERSION = 1


class StoreError(Exception):
    def __init__(self, message: str, retryable: bool = False) -> None:
        super().__init__(message)
        self.retryable = retryable


def _now() -> datetime:
    return datetime.now(timezone.utc)


def is_retryable_message(message: str) -> bool:
    return (
        message.startswith("Transaction conflict:")
        or "Transaction conflict:" in message
        or "not executed due to a failed transaction" in message
    )


@dataclass
class OutboxEvent:
    sequence: int
    aggregate_type: str
    aggregate_id: str
    event_type: str
    schema_version: int
    payload: dict[str, Any]
    occurred_at: datetime
    available_at: datetime


def outbox_draft(
    tenant: RecordID | None,
    aggregate_type: str,
    aggregate_id: str,
    event_type: str,
    schema_version: int,
    payload: dict[str, Any],
) -> dict[str, Any]:
    now = _now()
    return {
        "tenant": tenant,
        "aggregate_type": aggregate_type,
        "aggregate_id": aggregate_id,
        "event_type": event_type,
        "schema_version": schema_version,
        "payload": payload,
        "occurred_at": now,
        "available_at": now,
    }


class SurrealStore:
    """One authenticated SurrealDB connection with fully checked queries."""

    def __init__(self, connection: Any) -> None:
        self._db = connection
        self._lock = asyncio.Lock()

    @classmethod
    async def connect(
        cls,
        endpoint: str,
        namespace: str,
        database: str,
        username: str,
        password: str,
    ) -> "SurrealStore":
        db = AsyncSurreal(endpoint)
        await db.signin(
            {
                "namespace": namespace,
                "database": database,
                "username": username,
                "password": password,
            }
        )
        await db.use(namespace, database)
        return cls(db)

    @property
    def connection(self) -> Any:
        return self._db

    async def close(self) -> None:
        await self._db.close()

    async def query(self, sql: str, vars: dict[str, Any] | None = None) -> list[Any]:
        """Run a query and check EVERY statement result.

        The SDK's own `query` only checks the first statement, which silently
        swallows transaction failures; the Rust SDK's `.check()` checks all.
        """
        async with self._lock:
            response = await self._db.query_raw(sql, vars or {})
        if "error" in response and response["error"]:
            message = str(response["error"])
            raise StoreError(message, retryable=is_retryable_message(message))
        statements = response.get("result")
        if not isinstance(statements, list):
            raise StoreError(f"unexpected query response: {response!r}")
        results: list[Any] = []
        errors: list[str] = []
        for statement in statements:
            if statement.get("status") == "ERR":
                errors.append(str(statement.get("result")))
                results.append(None)
            else:
                results.append(statement.get("result"))
        if errors:
            message = "; ".join(errors)
            raise StoreError(message, retryable=is_retryable_message(message))
        return results

    async def query_with_retries(
        self, sql: str, vars: dict[str, Any] | None = None
    ) -> list[Any]:
        for attempt in range(MAX_TRANSACTION_ATTEMPTS):
            try:
                return await self.query(sql, vars)
            except StoreError as error:
                if error.retryable and attempt + 1 < MAX_TRANSACTION_ATTEMPTS:
                    await asyncio.sleep((1 << attempt) / 1_000)
                    continue
                raise
        raise AssertionError("the bounded retry loop always returns")

    async def ensure_identity(self, owner: TaskOwner) -> None:
        for attempt in range(MAX_TRANSACTION_ATTEMPTS):
            try:
                await self._ensure_identity_once(owner)
                return
            except StoreError as error:
                if error.retryable and attempt + 1 < MAX_TRANSACTION_ATTEMPTS:
                    await asyncio.sleep((1 << attempt) / 1_000)
                    continue
                raise

    async def _ensure_identity_once(self, owner: TaskOwner) -> None:
        tenant_key = owner.effective_tenant_key()
        enterprise_id = RecordID("enterprise", deterministic_enterprise_id())
        tenant_id = RecordID("tenant", deterministic_tenant_id(tenant_key))
        principal_id = RecordID(
            "principal", deterministic_principal_id(tenant_key, owner.principal_key)
        )
        now = _now()
        existing = await self.query(
            "SELECT * FROM ONLY $enterprise; SELECT * FROM ONLY $tenant; "
            "SELECT * FROM ONLY $principal;",
            {
                "enterprise": enterprise_id,
                "tenant": tenant_id,
                "principal": principal_id,
            },
        )
        existing_enterprise, existing_tenant, existing_principal = existing
        if existing_tenant is not None and (
            _record_key(existing_tenant["enterprise"]) != _record_key(enterprise_id)
            or existing_tenant["slug"] != tenant_key
        ):
            raise StoreError(f"conflicting identity for tenant `{tenant_key}`")
        if existing_principal is not None and (
            _record_key(existing_principal["tenant"]) != _record_key(tenant_id)
            or existing_principal["display_name"] != owner.principal_key
            or existing_principal["issuer"] != owner.issuer
            or existing_principal["subject"] != owner.subject
            or existing_principal["kind"] != owner.principal_kind.value
        ):
            raise StoreError(
                f"conflicting identity for principal `{owner.principal_key}`"
            )

        enterprise = existing_enterprise or {
            "id": enterprise_id,
            "slug": "installation",
            "name": "Veoveo installation",
            "enabled": True,
            "created_at": now,
        }
        enterprise["updated_at"] = now
        tenant = existing_tenant or {
            "id": tenant_id,
            "enterprise": enterprise_id,
            "slug": tenant_key,
            "name": tenant_key,
            "classification_ceiling": "installation_policy",
            "enabled": True,
            "created_at": now,
        }
        tenant["updated_at"] = now
        principal = existing_principal or {
            "id": principal_id,
            "tenant": tenant_id,
            "kind": owner.principal_kind.value,
            "issuer": owner.issuer,
            "subject": owner.subject,
            "email": None,
            "claims_hash": "",
            "enabled": True,
            "created_at": now,
        }
        principal["display_name"] = owner.principal_key
        principal["updated_at"] = now
        await self.query(
            "BEGIN TRANSACTION; "
            "UPSERT ONLY $enterprise CONTENT $enterprise_content RETURN NONE; "
            "UPSERT ONLY $tenant CONTENT $tenant_content RETURN NONE; "
            "UPSERT ONLY $principal CONTENT $principal_content RETURN NONE; "
            "COMMIT TRANSACTION;",
            {
                "enterprise": enterprise_id,
                "enterprise_content": enterprise,
                "tenant": tenant_id,
                "tenant_content": tenant,
                "principal": principal_id,
                "principal_content": principal,
            },
        )

    async def read_outbox(self, after_sequence: int, limit: int) -> list[OutboxEvent]:
        if limit == 0 or limit > MAX_OUTBOX_LIMIT:
            raise StoreError(f"outbox read limit must be 1..={MAX_OUTBOX_LIMIT}")
        rows = await self.query(
            "SELECT * FROM outbox_event WHERE sequence > $after AND "
            "available_at <= $now ORDER BY sequence ASC LIMIT $limit;",
            {"after": max(after_sequence, 0), "now": _now(), "limit": limit},
        )
        return [_outbox_event(row) for row in rows[0] or []]

    async def latest_available_outbox_sequence(self) -> int:
        rows = await self.query(
            "SELECT VALUE sequence FROM outbox_event WHERE available_at <= $now "
            "ORDER BY sequence DESC LIMIT 1;",
            {"now": _now()},
        )
        values = rows[0] or []
        return values[0] if values else 0

    async def outbox_wake(self) -> "OutboxWake":
        live_id = await self._db.live("outbox_event")
        stream = await self._db.subscribe_live(live_id)
        return OutboxWake(self._db, live_id, stream)

    async def upsert_domain_usage(
        self,
        task_id: uuid.UUID,
        server: str,
        model_id: str,
        kind: str,
        source_id: str | None = None,
        provider_job_id: str | None = None,
        quantity: float | None = None,
        unit: str | None = None,
        amount: float | None = None,
        currency: str | None = None,
        metadata: dict[str, Any] | None = None,
        recorded_at: datetime | None = None,
    ) -> None:
        task_rows = await self.query(
            "SELECT * FROM ONLY $task;", {"task": RecordID("task", task_id)}
        )
        task = task_rows[0]
        if task is None:
            raise StoreError(f"task `{task_id}` was not found")
        if _record_key(task["server"]) != server:
            raise StoreError(f"task `{task_id}` does not belong to server `{server}`")
        usage_key = "|".join(
            [server, str(task_id), kind, model_id, source_id or "", provider_job_id or ""]
        )
        usage_id = RecordID("domain_usage", uuid.uuid5(uuid.NAMESPACE_OID, usage_key))
        now = recorded_at or _now()
        content = {
            "tenant": task["tenant"],
            "task": RecordID("task", task_id),
            "server": server_record(server),
            "source_id": source_id,
            "provider_job_id": provider_job_id,
            "model_id": model_id,
            "kind": kind,
            "quantity": quantity,
            "unit": unit,
            "amount": amount,
            "currency": currency,
            "metadata": metadata or {},
            "recorded_at": now,
            "updated_at": _now(),
        }
        event = outbox_draft(
            task["tenant"],
            "domain_usage",
            str(usage_id.id),
            "domain.usage.recorded",
            DOMAIN_USAGE_EVENT_SCHEMA_VERSION,
            {
                "task_id": str(task_id),
                "server": server,
                "model_id": model_id,
                "kind": kind,
            },
        )
        await self.query_with_retries(
            "BEGIN TRANSACTION; UPSERT ONLY $usage CONTENT $content RETURN NONE; "
            "CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;",
            {"usage": usage_id, "content": content, "outbox": event},
        )

    async def domain_usage_for_task(
        self, server: str, task_id: uuid.UUID
    ) -> list[dict[str, Any]]:
        rows = await self.query(
            "SELECT * FROM domain_usage WHERE server = $server AND task = $task "
            "ORDER BY recorded_at ASC, id ASC;",
            {"server": server_record(server), "task": RecordID("task", task_id)},
        )
        return rows[0] or []

    async def domain_usage_task_ids(self, server: str) -> list[uuid.UUID]:
        rows = await self.query(
            "SELECT VALUE task FROM domain_usage WHERE server = $server "
            "GROUP BY task ORDER BY task ASC;",
            {"server": server_record(server)},
        )
        return [parse_task_id(_record_key(record)) for record in rows[0] or []]


class OutboxWake:
    """LIVE-query wake signal over the outbox; latency only, never correctness."""

    def __init__(self, db: Any, live_id: Any, stream: AsyncIterator[Any]) -> None:
        self._db = db
        self._live_id = live_id
        self._stream = stream

    async def wait(self, timeout_seconds: float) -> None:
        try:
            await asyncio.wait_for(anext(self._stream), timeout=timeout_seconds)
        except (TimeoutError, StopAsyncIteration):
            pass

    async def close(self) -> None:
        try:
            await self._db.kill(self._live_id)
        except Exception:  # noqa: BLE001 — teardown only
            pass


def _record_key(record: Any) -> str:
    if isinstance(record, RecordID):
        return str(record.id)
    if isinstance(record, str):
        _, _, key = record.partition(":")
        return key or record
    raise InvalidRecord(f"unsupported record key {record!r}")


def _outbox_event(row: dict[str, Any]) -> OutboxEvent:
    return OutboxEvent(
        sequence=row["sequence"],
        aggregate_type=row["aggregate_type"],
        aggregate_id=row["aggregate_id"],
        event_type=row["event_type"],
        schema_version=row["schema_version"],
        payload=row["payload"],
        occurred_at=row["occurred_at"],
        available_at=row["available_at"],
    )
