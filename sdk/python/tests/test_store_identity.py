"""Identity reuse preserves presentation metadata and current security fields."""

import asyncio
import uuid

import pytest

from query_files import test_query
from veoveo_mcp.tasks import StoreError
from test_owner_task_query import runtime  # noqa: F401
from test_task_runtime_integration import owner


async def test_identity_reuses_a_named_principal_without_rewriting_metadata(runtime):
    async with asyncio.timeout(15):
        caller = owner(f"identity-name-{uuid.uuid4()}")
        await runtime.store.ensure_identity(caller)
        await runtime.store.query(
            test_query("test_store_identity/test_identity_reuses_a_named_principal_without_rewriting_metadata.surql"),
            {"principal": caller.principal_record()},
        )
        before = await runtime.store.query(test_query("test_store_identity/test_identity_reuses_a_named_principal_without_rewriting_metadata_2.surql"), {"principal": caller.principal_record()})
        await runtime.store.ensure_identity(caller)
        after = await runtime.store.query(test_query("test_store_identity/test_identity_reuses_a_named_principal_without_rewriting_metadata_2.surql"), {"principal": caller.principal_record()})
        assert after == before


@pytest.mark.parametrize(
    'assignment,conflict',
    [
        ('test_store_identity/intercept/mutation_01.surql', False),
        ('test_store_identity/intercept/mutation_02.surql', True),
        ('test_store_identity/intercept/mutation_03.surql', True),
        ('test_store_identity/intercept/mutation_04.surql', True),
    ],
)
async def test_identity_transaction_checks_current_fields_without_stale_overwrite(runtime, monkeypatch, assignment, conflict):
    async with asyncio.timeout(15):
        caller = owner(f"identity-race-{uuid.uuid4()}")
        store = runtime.store
        await store.ensure_identity(caller)
        query = store.query
        raced = False
        expected = None

        async def intercept(sql, bindings=None):
            nonlocal raced, expected
            if "BEGIN TRANSACTION" in sql and not raced:
                raced = True
                # A concurrent policy writer settles immediately before the identity transaction.
                await query(test_query(assignment), {"principal": caller.principal_record()})
                expected = await query(test_query("test_store_identity/intercept.surql"), {"principal": caller.principal_record()})
            return await query(sql, bindings)

        monkeypatch.setattr(store, "query", intercept)
        if conflict:
            with pytest.raises(StoreError, match="identity_principal_conflict"):
                await store.ensure_identity(caller)
        else:
            await store.ensure_identity(caller)
        assert raced
        assert await query(test_query("test_store_identity/test_identity_transaction_checks_current_fields_without_stale_overwrite.surql"), {"principal": caller.principal_record()}) == expected
