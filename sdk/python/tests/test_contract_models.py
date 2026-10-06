import uuid
from datetime import datetime, timezone

import pytest
from uuid_utils.compat import uuid7

from veoveo_mcp.contract import (
    ArtifactMetadata,
    IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability,
    PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
    UsageKind,
    UsageRecord,
    UsageReport,
)


def test_output_capability_carries_bounded_inherited_labels():
    request = IssueArtifactWriteCapabilityRequest(
        task_id=str(uuid7()),
        expires_at=datetime.now(timezone.utc),
        max_artifact_count=2,
        max_total_bytes=1024,
        required_data_labels={"retained-home"},
    )
    assert request.model_dump(mode="json")["required_data_labels"] == ["retained-home"]
    with pytest.raises(ValueError):
        IssueArtifactWriteCapabilityRequest.model_validate(
            {**request.model_dump(), "required_labels": ["retained-home"]}
        )
    with pytest.raises(ValueError):
        IssueArtifactWriteCapabilityRequest.model_validate(
            {**request.model_dump(), "required_data_labels": {f"label-{i}" for i in range(257)}}
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
    v7 = str(uuid7())
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
    capability_id = str(uuid7())
    task_id = str(uuid7())
    issued = IssuedArtifactWriteCapability(
        capability_id=capability_id,
        secret="s" * 32,
        task_id=task_id,
        expires_at=datetime.now(timezone.utc),
    )
    assert "s" * 32 not in repr(issued)
    with pytest.raises(Exception):
        IssuedArtifactWriteCapability(
            capability_id=capability_id,
            secret="short",
            task_id=task_id,
            expires_at=datetime.now(timezone.utc),
        )
    redemption = RedeemArtifactWriteCapabilityRequest(
        capability_id=capability_id,
        task_id=task_id,
        idempotency_key="datasheet:task-1:report",
        artifact=PutArtifactRequest(),
    )
    assert redemption.wire()["idempotency_key"] == "datasheet:task-1:report"
    with pytest.raises(Exception):
        RedeemArtifactWriteCapabilityRequest(
            capability_id=capability_id,
            task_id=task_id,
            idempotency_key=" leading",
            artifact=PutArtifactRequest(),
        )


@pytest.mark.parametrize("model_name,payload", [
    ("PrincipalAccessSubject", {"kind": "principal", "id": "alice"}),
    ("GroupAccessSubject", {"kind": "group", "id": "team"}),
    ("WorkContextGrant", {"subject": {"kind": "principal", "id": "alice"}, "level": "read"}),
    ("WorkContextOutputPolicy", {"owner": {"kind": "group", "id": "team"}}),
    ("DirectInvocationProvenance", {"mode": "direct", "initiator": "alice"}),
    ("DelegatedInvocationProvenance", {"mode": "delegated", "initiator": "alice", "delegation_id": "grant-1"}),
    ("AutomatedInvocationProvenance", {"mode": "automated"}),
    ("InvocationAuthority", {
        "work_context": "context", "tenant": "tenant", "membership": "owner",
        "policy_revision": "v1", "output_policy": {"owner": {"kind": "principal", "id": "alice"}},
        "provenance": {"mode": "automated"},
    }),
])
def test_authority_models_preserve_valid_wire_and_reject_unknown_fields(model_name, payload):
    from veoveo_mcp.contract import identity

    model = getattr(identity, model_name)
    admitted = model.model_validate(payload)
    assert model.model_validate(admitted.model_dump(mode="json")) == admitted
    with pytest.raises(ValueError):
        model.model_validate({**payload, "unexpected": True})


def test_normalized_principal_and_group_membership_reject_unknown_fields():
    from veoveo_mcp.contract.identity import GroupMembership, Principal

    principal = {"id": "alice", "kind": "user", "issuer": "https://idp.example.com", "subject": "alice"}
    admitted = Principal.model_validate(principal)
    assert Principal.model_validate(admitted.model_dump()) == admitted
    with pytest.raises(ValueError, match="extra_forbidden"):
        Principal.model_validate({**principal, "external_claim": True})
    with pytest.raises(ValueError, match="extra_forbidden"):
        GroupMembership.model_validate({"group": "engineering", "role": "read", "unexpected": True})


def test_normalized_principal_assurances_use_the_closed_rust_vocabulary():
    from veoveo_mcp.contract import Principal, PrincipalAssurance

    principal = {"id": "alice", "kind": "user", "issuer": "https://idp.example.com", "subject": "alice"}
    assert Principal.model_validate(principal).assurances == set()
    for assurances in [
        ["us_person"],
        [PrincipalAssurance.US_PERSON],
        ["us_person", "us_person"],
    ]:
        admitted = Principal.model_validate({**principal, "assurances": assurances})
        assert admitted.assurances == {PrincipalAssurance.US_PERSON}
        assert admitted.model_dump(mode="json")["assurances"] == ["us_person"]
        assert Principal.model_validate_json(admitted.model_dump_json()) == admitted
    assert list(PrincipalAssurance) == [PrincipalAssurance.US_PERSON]
    assert Principal.model_json_schema()["$defs"]["PrincipalAssurance"]["enum"] == ["us_person"]
    for unknown in ["contractor", "UsPerson", "US_PERSON", "", "us_person "]:
        with pytest.raises(ValueError, match="enum"):
            Principal.model_validate({**principal, "assurances": [unknown]})


@pytest.mark.parametrize("change", [
    {"byte_len": -1}, {"byte_len": 2**64}, {"byte_len": 0.5}, {"byte_len": True},
    {"release_state": "invented"},
    {"created_at": "2026-01-01T00:00:00"},
    {"artifact_uri": "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4472"},
    {"compliance": {"owner": {"kind": "unknown", "id": "alice"}}},
    {"compliance": {"work_context": " bad"}},
    {"compliance": {"provenance": {"producer": "issuer#worker", "invocation_mode": "delegated", "policy_revision": "v1"}}},
])
def test_artifact_known_fields_and_copy_share_admission(change):
    metadata = ArtifactMetadata(
        artifact_id="0197f78e-f2f0-7a6e-8a5d-f41c691e4471", byte_len=0,
        artifact_uri="artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
        created_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
    )
    with pytest.raises(ValueError):
        ArtifactMetadata.model_validate({**metadata.model_dump(), **change})
    with pytest.raises(ValueError):
        metadata.model_copy(update=change)
    with pytest.raises(ValueError):
        metadata.artifact_uri = "artifact://invalid"
    assert metadata.presented_under_scheme("datasheet").artifact_id == metadata.artifact_id
    assert metadata.model_copy(update={"byte_len": 2**64 - 1}).byte_len == 2**64 - 1


@pytest.mark.parametrize("quantity,amount", [(float("inf"), 0), (0, float("nan"))])
def test_usage_record_rejects_nonfinite_values(quantity, amount):
    value = _record(UsageKind.ACTUAL, 0.0, "USD").model_dump()
    with pytest.raises(ValueError):
        UsageRecord.model_validate({**value, "quantity": quantity, "amount": amount})


def test_usage_report_rejects_foreign_parents_and_detached_totals():
    record = _record(UsageKind.ACTUAL, -0.25, "USD")
    report = UsageReport.build("task-1", "datasheet://usage/task/task-1", [record])
    assert report.total_amount == -0.25
    for change in [{"task_id": "other"}, {"total_amount": 0.25}, {"total_kind": "estimate"}, {"currency": "EUR"}]:
        with pytest.raises(ValueError):
            UsageReport.model_validate({**report.model_dump(), **change})
        with pytest.raises(ValueError):
            report.model_copy(update=change)
    assert UsageReport.model_validate_json(report.model_dump_json()) == report


def test_artifact_capability_native_task_profile_and_unsigned_budgets():
    from veoveo_mcp.contract import ArtifactTaskId
    task = "0197f78e-f2f0-7a6e-8a5d-f41c691e4471"
    assert ArtifactTaskId(task.upper()) == task
    for invalid in [str(uuid.uuid4()), "task-1", "0197f78e-f2f0-7a6e-0a5d-f41c691e4471"]:
        with pytest.raises(ValueError):
            ArtifactTaskId(invalid)
    value = dict(task_id=task, expires_at=datetime.now(timezone.utc), max_artifact_count=1, max_total_bytes=1)
    for change in [{"max_artifact_count": 2**32}, {"max_total_bytes": 2**64}, {"max_total_bytes": True}]:
        with pytest.raises(ValueError):
            IssueArtifactWriteCapabilityRequest(**{**value, **change})


def test_rust_serialized_artifact_fixture_and_opaque_extensions():
    import json
    from pathlib import Path
    fixture = Path(__file__).resolve().parents[3] / "platform/artifacts/contract/tests/fixtures/metadata-output.json"
    wire = json.loads(fixture.read_text())
    metadata = ArtifactMetadata.model_validate(wire)
    assert json.loads(metadata.model_dump_json()) == wire
    assert metadata.compliance.owner.kind == "group"
    assert metadata.compliance.provenance.invocation_mode == "delegated"
    assert metadata.metadata == {"producer_annotation": [1, "open metadata"]}
    extra = ArtifactMetadata.model_validate({**wire, "upstream_extension": {"open": True}})
    assert extra.model_dump()["upstream_extension"] == {"open": True}
