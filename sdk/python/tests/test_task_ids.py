"""RFC 9562 generation, native driver values, and same-millisecond ordering."""

import time
import uuid

from pydantic import TypeAdapter, UUID7

from veoveo_mcp.tasks import new_task_id, parse_task_id
from veoveo_mcp.tasks.types import task_record


def test_task_uuid7_uses_the_rfc_millisecond_timestamp_and_native_uuid_type():
    before_ms = time.time_ns() // 1_000_000
    ids = [new_task_id() for _ in range(4096)]
    after_ms = time.time_ns() // 1_000_000
    assert len(set(ids)) == len(ids)
    assert ids == sorted(ids)
    assert all(type(value) is uuid.UUID for value in ids)
    assert all(value.version == 7 and value.variant == uuid.RFC_4122 for value in ids)
    assert all(before_ms <= value.int >> 80 <= after_ms for value in ids)

    validator = TypeAdapter(UUID7)
    for value in (ids[0], ids[-1]):
        assert validator.validate_python(value) == value
        assert parse_task_id(str(value)) == value
        record = task_record(value)
        assert record.table_name == "task"
        assert type(record.id) is uuid.UUID and record.id == value
