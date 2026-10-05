"""SurrealDB platform-store access for Python MCP servers.

A focused port of the `veoveo-platform-store` surfaces the task runtime and
domain servers need: checked multi-statement queries, canonical identity
upserts, native LIVE wakeups, and domain usage. Schema lanes belong to their Rust owners and the module runner installs them;
this module reads and writes that schema with the database-level runtime user.
"""

from __future__ import annotations

import asyncio
import contextlib
import re
import uuid
from collections.abc import AsyncGenerator, Awaitable, Callable
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any

from surrealdb import AsyncSurreal, RecordID
from surrealdb.cbor import CBORSimpleValue

from .queries import query
from .types import (
    InvalidRecord,
    TaskOwner,
    TaskResult,
    deterministic_enterprise_id,
    deterministic_principal_id,
    deterministic_tenant_id,
    server_record,
)

MAX_TRANSACTION_ATTEMPTS = 8

ConnectionFactory = Callable[[], Awaitable[Any]]


class StoreError(Exception):
    def __init__(self, message: str, retryable: bool = False) -> None:
        super().__init__(message)
        self.retryable = retryable


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _json_to_surreal(value: Any) -> Any:
    """Match the Rust Store's JSON null and unsigned-integer representation."""
    if value is None:
        # The pinned SDK encodes ordinary None as database NONE.
        return CBORSimpleValue(22)
    if isinstance(value, int) and value > 2**63 - 1:
        return Decimal(value)
    if isinstance(value, dict):
        return {key: _json_to_surreal(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_json_to_surreal(item) for item in value]
    return value


def _json_from_surreal(value: Any) -> Any:
    if isinstance(value, Decimal):
        return int(value) if value == value.to_integral_value() else float(value)
    if isinstance(value, dict):
        return {key: _json_from_surreal(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_json_from_surreal(item) for item in value]
    return value


def task_result_to_store(result: TaskResult | None) -> dict[str, Any] | None:
    if result is None:
        return None
    return {"payload": _json_to_surreal(result.payload)}


def task_result_from_store(value: Any) -> TaskResult | None:
    if value is None:
        return None
    if not isinstance(value, dict) or set(value) != {"payload"}:
        raise InvalidRecord("invalid stored Task result envelope")
    return TaskResult(_json_from_surreal(value["payload"]))


def is_retryable_message(message: str) -> bool:
    return (
        message.startswith("Transaction conflict:")
        or "Transaction conflict:" in message
        or "not executed due to a failed transaction" in message
    )


class SurrealStore:
    """One authenticated SurrealDB connection with fully checked queries."""

    def __init__(
        self,
        connection: Any,
        connection_factory: ConnectionFactory | None = None,
    ) -> None:
        self._db = connection
        self._connection_factory = connection_factory
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
        async def open_connection() -> Any:
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
            return db

        return cls(await open_connection(), open_connection)

    @property
    def connection(self) -> Any:
        return self._db

    async def close(self) -> None:
        async with self._lock:
            await self._db.close()

    async def query(self, sql: str, vars: dict[str, Any] | None = None) -> list[Any]:
        """Run a query and check EVERY statement result.

        The SDK's own `query` only checks the first statement, which silently
        swallows transaction failures; the Rust SDK's `.check()` checks all.
        """
        async with self._lock:
            await self._replace_stale_connection()
            try:
                response = await self._db.query_raw(sql, vars or {})
            except asyncio.CancelledError as error:
                current = asyncio.current_task()
                if current is not None and current.cancelling():
                    raise
                await self._restore_connection()
                raise StoreError(
                    "SurrealDB connection closed; connection restored for the "
                    "next request"
                ) from error
            except Exception as error:
                if not self._connection_is_stale():
                    raise
                await self._restore_connection()
                raise StoreError(
                    "SurrealDB connection closed; connection restored for the "
                    "next request"
                ) from error
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
        # Match the Store's identity fields; display names are presentation metadata.
        segments = [part.strip() for part in re.split(r"[#/]", owner.subject) if part.strip()]
        display_name = segments[-1][:128] if segments else "Principal"
        await self.query(
            query("store/_ensure_identity_once.surql"),
            {
                "enterprise": enterprise_id,
                "enterprise_content": {
                    "id": enterprise_id, "slug": "installation", "name": "Veoveo installation",
                    "enabled": True, "created_at": now, "updated_at": now,
                },
                "tenant": tenant_id,
                "tenant_content": {
                    "id": tenant_id, "enterprise": enterprise_id, "slug": tenant_key,
                    "name": tenant_key, "classification_ceiling": "installation_policy",
                    "enabled": True, "created_at": now, "updated_at": now,
                },
                "principal": principal_id,
                "principal_content": {
                    "id": principal_id, "tenant": tenant_id, "kind": owner.principal_kind.value,
                    "issuer": owner.issuer, "subject": owner.subject, "display_name": display_name,
                    "email": None, "claims_hash": "", "enabled": True,
                    "created_at": now, "updated_at": now,
                },
            },
        )

    async def task_wake(self) -> "NativeWake":
        async with self._lock:
            await self._replace_stale_connection()
            live_id = await self._db.query(query("store/task_wake.surql"))
            stream = await self._db.subscribe_live(live_id)
            return NativeWake(self._db, live_id, stream)

    def _connection_is_stale(self) -> bool:
        if not hasattr(self._db, "socket"):
            return False
        receive_task = getattr(self._db, "recv_task", None)
        socket = getattr(self._db, "socket", None)
        return socket is None or (receive_task is not None and receive_task.done())

    async def _replace_stale_connection(self) -> None:
        if self._connection_is_stale():
            await self._restore_connection()

    async def _restore_connection(self) -> None:
        if self._connection_factory is None:
            raise StoreError("SurrealDB connection closed and cannot be restored")
        previous = self._db
        self._db = await self._connection_factory()
        with contextlib.suppress(Exception):
            await previous.close()

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
            query("store/upsert_domain_usage.surql"),
            {"task": RecordID("task", task_id), "server": server_record(server)},
        )
        if not task_rows[0]:
            raise StoreError(f"task `{task_id}` was not found for server `{server}`")
        task = task_rows[0][0]
        usage_key = "|".join(
            [
                server,
                str(task_id),
                kind,
                model_id,
                source_id or "",
                provider_job_id or "",
            ]
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
        await self.query_with_retries(
            query("store/upsert_domain_usage_2.surql"),
            {"usage": usage_id, "content": content},
        )


class NativeWake:
    """One owned LIVE reader; idle deadlines leave its next event pending."""

    def __init__(
        self, db: Any, live_id: Any, stream: AsyncGenerator[Any, None]
    ) -> None:
        self._db = db
        self._live_id = live_id
        self._stream = stream
        self._receive = getattr(db, "recv_task", None)
        self._pending: asyncio.Future[Any] | None = None
        self._closed = False

    async def wait(self, timeout_seconds: float | None = None) -> None:
        if self._closed:
            raise StoreError("LIVE query wake is closed")
        if self._pending is None:
            self._pending = asyncio.ensure_future(anext(self._stream))
        pending = self._pending
        try:
            receive = self._receive
            sources = {pending}
            if receive is not None:
                sources.add(receive)
            ready, _ = await asyncio.wait(
                sources, timeout=timeout_seconds, return_when=asyncio.FIRST_COMPLETED
            )
            if receive is not None and receive in ready:
                raise StoreError(
                    "LIVE connection ended; reconnect and renew the subscription",
                    retryable=True,
                )
            if pending in ready:
                pending.result()
        except StopAsyncIteration as error:
            raise StoreError(
                "LIVE query stream ended; reconnect and renew the subscription",
                retryable=True,
            ) from error
        finally:
            if pending.done():
                self._pending = None

    async def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        pending, self._pending = self._pending, None
        try:
            async with asyncio.timeout(5):
                if pending is not None:
                    pending.cancel()
                    with contextlib.suppress(asyncio.CancelledError):
                        await pending
                await self._stream.aclose()
        except Exception:  # noqa: BLE001 — teardown only
            pass
        finally:
            try:
                if self._receive is None or not self._receive.done():
                    async with asyncio.timeout(5):
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
