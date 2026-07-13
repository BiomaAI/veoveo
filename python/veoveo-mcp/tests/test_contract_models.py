import uuid
from datetime import datetime, timezone

import pytest
import uuid_extensions

from veoveo_mcp.contract import (
    ArtifactMetadata,
    IssuedArtifactWriteCapability,
    PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
    UsageKind,
    UsageRecord,
    UsageReport,
)


def _record(kind: UsageKind, amount: float | None, currency: str | None) -> UsageRecord:
    return UsageRecord(
        task_id="task-1",
        model_id="model",
        kind=kind,
        quantity=1.0,
        unit="run",
        amount=amount,
        currency=currency,
        recorded_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
    )


def test_usage_report_totals_common_currency():
    report = UsageReport.build(
        "task-1",
        "datasheet://usage/task/task-1",
        [
            _record(UsageKind.ESTIMATE, 0.25, "USD"),
            _record(UsageKind.ACTUAL, 0.25, "USD"),
        ],
    )
    assert report.total_kind == "actual"
    assert report.total_amount == 0.25
    assert report.currency == "USD"


def test_usage_report_without_records_has_no_totals():
    report = UsageReport.build("task-1", "datasheet://usage/task/task-1", [])
    assert report.total_kind is None
    assert report.total_amount is None


def test_artifact_ids_must_be_uuid_v7():
    v7 = str(uuid_extensions.uuid7())
    metadata = ArtifactMetadata(
        artifact_id=v7,
        byte_len=3,
        artifact_uri=f"artifact://{v7}",
        created_at=datetime.now(timezone.utc),
    )
    assert metadata.presented_under_scheme("datasheet").artifact_uri == (
        f"datasheet://artifact/{v7}"
    )
    with pytest.raises(Exception):
        ArtifactMetadata(
            artifact_id=str(uuid.uuid4()),
            byte_len=3,
            artifact_uri="artifact://x",
            created_at=datetime.now(timezone.utc),
        )


def test_put_request_wire_skips_unset_fields_like_rust_serde():
    assert PutArtifactRequest().wire() == {}
    wire = PutArtifactRequest(
        mime_type="application/json",
        filename="report.json",
        data_labels={"cui"},
        metadata={"task_id": "t"},
    ).wire()
    assert wire == {
        "mime_type": "application/json",
        "filename": "report.json",
        "data_labels": ["cui"],
        "metadata": {"task_id": "t"},
    }


def test_capability_secret_is_validated_and_redacted():
    capability_id = str(uuid_extensions.uuid7())
    issued = IssuedArtifactWriteCapability(
        capability_id=capability_id,
        secret="s" * 32,
        task_id="task-1",
        expires_at=datetime.now(timezone.utc),
    )
    assert "s" * 32 not in repr(issued)
    with pytest.raises(Exception):
        IssuedArtifactWriteCapability(
            capability_id=capability_id,
            secret="short",
            task_id="task-1",
            expires_at=datetime.now(timezone.utc),
        )
    redemption = RedeemArtifactWriteCapabilityRequest(
        capability_id=capability_id,
        task_id="task-1",
        idempotency_key="datasheet:task-1:report",
        artifact=PutArtifactRequest(),
    )
    assert redemption.wire()["idempotency_key"] == "datasheet:task-1:report"
    with pytest.raises(Exception):
        RedeemArtifactWriteCapabilityRequest(
            capability_id=capability_id,
            task_id="task-1",
            idempotency_key=" leading",
            artifact=PutArtifactRequest(),
        )
