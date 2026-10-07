import uuid
from datetime import datetime, timezone

import pytest
from uuid_utils.compat import uuid7

from veoveo_mcp.types import ChronoTimestamp

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
        taskId=str(uuid7()),
        expiresAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)),
        maxArtifactCount=2,
        maxTotalBytes=1024,
        requiredDataLabels={"retained-home"},
    )
    assert request.model_dump(mode="json")["requiredDataLabels"] == ["retained-home"]
    with pytest.raises(ValueError):
        IssueArtifactWriteCapabilityRequest.model_validate(
            {**request.model_dump(), "required_labels": ["retained-home"]}
        )
    with pytest.raises(ValueError):
        IssueArtifactWriteCapabilityRequest.model_validate(
            {**request.model_dump(), "requiredDataLabels": {f"label-{i}" for i in range(257)}}
        )


def _record(kind: UsageKind, amount: float | None, currency: str | None) -> UsageRecord:
    return UsageRecord(
        taskId="task-1",
        modelId="model",
        kind=kind,
        quantity=1.0,
        unit="run",
        amount=amount,
        currency=currency,
        recordedAt=datetime(2026, 1, 1, tzinfo=timezone.utc),
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
        artifactId=v7,
        byteLen=3,
        artifactUri=f"artifact://{v7}",
        createdAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)),
    )
    assert metadata.presented_under_scheme("datasheet").artifact_uri == (
        f"datasheet://artifact/{v7}"
    )
    with pytest.raises(Exception):
        ArtifactMetadata(
            artifactId=str(uuid.uuid4()),
            byteLen=3,
            artifactUri="artifact://x",
            createdAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)),
        )


def test_put_request_wire_skips_unset_fields_like_rust_serde():
    assert PutArtifactRequest().wire() == {}
    wire = PutArtifactRequest(
        mimeType="application/json",
        filename="report.json",
        dataLabels={"cui"},
        metadata={"taskId": "t"},
    ).wire()
    assert wire == {
        "mimeType": "application/json",
        "filename": "report.json",
        "dataLabels": ["cui"],
        "metadata": {"taskId": "t"},
    }


def test_capability_secret_is_validated_and_redacted():
    capability_id = str(uuid7())
    task_id = str(uuid7())
    issued = IssuedArtifactWriteCapability(
        capabilityId=capability_id,
        secret="s" * 32,
        taskId=task_id,
        expiresAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)),
    )
    assert "s" * 32 not in repr(issued)
    with pytest.raises(Exception):
        IssuedArtifactWriteCapability(
            capabilityId=capability_id,
            secret="short",
            taskId=task_id,
            expiresAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)),
        )
    redemption = RedeemArtifactWriteCapabilityRequest(
        capabilityId=capability_id,
        taskId=task_id,
        idempotencyKey="datasheet:task-1:report",
        artifact=PutArtifactRequest(),
    )
    assert redemption.wire()["idempotencyKey"] == "datasheet:task-1:report"
    with pytest.raises(Exception):
        RedeemArtifactWriteCapabilityRequest(
            capabilityId=capability_id,
            taskId=task_id,
            idempotencyKey=" leading",
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
    {"byteLen": -1}, {"byteLen": 2**64}, {"byteLen": 0.5}, {"byteLen": True},
    {"releaseState": "invented"},
    {"createdAt": "2026-01-01T00:00:00"},
    {"artifactUri": "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4472"},
    {"compliance": {"owner": {"kind": "unknown", "id": "alice"}}},
    {"compliance": {"workContext": " bad"}},
    {"compliance": {"provenance": {"producer": "issuer#worker", "invocationMode": "delegated", "policyRevision": "v1"}}},
])
def test_artifact_known_fields_and_copy_share_admission(change):
    metadata = ArtifactMetadata(
        artifactId="0197f78e-f2f0-7a6e-8a5d-f41c691e4471", byteLen=0,
        artifactUri="artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
        createdAt=ChronoTimestamp.from_datetime(datetime(2026, 1, 1, tzinfo=timezone.utc)),
    )
    with pytest.raises(ValueError):
        ArtifactMetadata.model_validate({**metadata.model_dump(), **change})
    with pytest.raises(ValueError):
        metadata.model_copy(update=change)
    with pytest.raises(ValueError):
        metadata.artifact_uri = "artifact://invalid"
    assert metadata.presented_under_scheme("datasheet").artifact_id == metadata.artifact_id
    assert metadata.model_copy(update={"byteLen": 2**64 - 1}).byte_len == 2**64 - 1


@pytest.mark.parametrize("quantity,amount", [(float("inf"), 0), (0, float("nan"))])
def test_usage_record_rejects_nonfinite_values(quantity, amount):
    value = _record(UsageKind.ACTUAL, 0.0, "USD").model_dump()
    with pytest.raises(ValueError):
        UsageRecord.model_validate({**value, "quantity": quantity, "amount": amount})


def test_usage_report_rejects_foreign_parents_and_detached_totals():
    record = _record(UsageKind.ACTUAL, -0.25, "USD")
    report = UsageReport.build("task-1", "datasheet://usage/task/task-1", [record])
    assert report.total_amount == -0.25
    for change in [{"taskId": "other"}, {"totalAmount": 0.25}, {"totalKind": "estimate"}, {"currency": "EUR"}]:
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
    value = dict(taskId=task, expiresAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)), maxArtifactCount=1, maxTotalBytes=1)
    for change in [{"maxArtifactCount": 2**32}, {"maxTotalBytes": 2**64}, {"maxTotalBytes": True}]:
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
    with pytest.raises(ValueError):
        ArtifactMetadata.model_validate({**wire, "upstream_extension": {"open": True}})
    for old in ["artifact_id", "artifact_uri", "byte_len", "download_url", "created_at"]:
        with pytest.raises(ValueError):
            ArtifactMetadata.model_validate({**wire, old: None})
    for old in ["invocation_mode", "delegation_id", "policy_revision"]:
        mixed = {**wire, "compliance": {**wire["compliance"], "provenance": {**wire["compliance"]["provenance"], old: None}}}
        with pytest.raises(ValueError):
            ArtifactMetadata.model_validate(mixed)
    with pytest.raises(ValueError):
        PutArtifactRequest.model_validate({"mimeType": "text/plain", "mime_type": "text/plain"})


def test_changed_wire_families_refuse_retired_keys_in_both_decoders():
    import json
    from veoveo_mcp.contract.artifacts import ArtifactProvenance, ComplianceMetadata

    identity = str(uuid7())
    timestamp = "2026-10-06T00:00:00Z"
    provenance = {"producer":"producer", "invocationMode":"direct", "initiator":"caller",
                  "policyRevision":"policy-1"}
    compliance = {"tenantId":"tenant", "workContext":"context", "dataLabels":[],
                  "retentionExpiresAt":timestamp, "provenance":provenance}
    artifact = {"artifactId":identity, "artifactUri":f"artifact://{identity}", "byteLen":3,
                "mimeType":"text/plain", "createdAt":timestamp, "releaseState":"private",
                "downloadUrl":"https://example.test/blob", "compliance":compliance,
                "metadata":{"provider_owned_field":True}}
    cases = [
        (ArtifactProvenance, provenance),
        (ComplianceMetadata, compliance),
        (ArtifactMetadata, artifact),
        (PutArtifactRequest, {"mimeType":"text/plain", "dataLabels":[], "retentionExpiresAt":timestamp}),
        (IssueArtifactWriteCapabilityRequest, {"taskId":identity, "expiresAt":timestamp,
            "maxArtifactCount":1, "maxTotalBytes":3, "requiredDataLabels":[]}),
        (IssuedArtifactWriteCapability, {"capabilityId":identity, "secret":"s"*32,
            "taskId":identity, "expiresAt":timestamp}),
        (RedeemArtifactWriteCapabilityRequest, {"capabilityId":identity, "taskId":identity,
            "idempotencyKey":"fixture:write", "artifact":{"mimeType":"text/plain"}}),
        (UsageRecord, {"taskId":"task", "sourceId":"source", "providerJobId":"job",
            "modelId":"model", "kind":"actual", "recordedAt":timestamp,
            "metadata":{"provider_owned_field":True}}),
        (UsageReport, {"taskId":"task", "usageUri":"fixture://usage/task", "records":[],
            "totalKind":None, "totalAmount":None}),
    ]
    for model, current in cases:
        assert model.model_validate(current).model_dump(mode="json") == model.model_validate_json(json.dumps(current)).model_dump(mode="json")
        for name, field in model.model_fields.items():
            alias = field.alias or name
            if alias == name or alias not in current:
                continue
            for replacement in (False, True):
                invalid = dict(current)
                invalid[name] = invalid[alias]
                if replacement:
                    del invalid[alias]
                for decode, payload in ((model.model_validate, invalid),
                                        (model.model_validate_json, json.dumps(invalid))):
                    with pytest.raises(ValueError):
                        decode(payload)
    nested = {**artifact, "compliance":{**compliance, "work_context":"context"}}
    for decode, payload in ((ArtifactMetadata.model_validate, nested),
                            (ArtifactMetadata.model_validate_json, json.dumps(nested))):
        with pytest.raises(ValueError):
            decode(payload)


def test_upload_receipt_nominal_identity_and_current_wire_admission():
    from pydantic import TypeAdapter
    from veoveo_mcp.contract import ArtifactUploadId, ArtifactUploadReceipt
    from copy import deepcopy
    import json

    upload_id, artifact_id = str(uuid7()), str(uuid7())
    current = dict(uploadId=upload_id, artifactId=artifact_id,
                   artifactUri=f"artifact://{artifact_id}", sha256="a" * 64,
                   byteLen=1, mimeType="application/octet-stream", filename="fixture.bin",
                   createdAt="2026-01-01T00:00:00Z")
    adapter = TypeAdapter(ArtifactUploadReceipt)
    def decoders(value):
        return [lambda: ArtifactUploadReceipt.model_validate(value),
                lambda: ArtifactUploadReceipt.model_validate_json(json.dumps(value)),
                lambda: adapter.validate_python(value),
                lambda: adapter.validate_json(json.dumps(value))]
    for decode in decoders(current):
        receipt = decode()
        assert isinstance(receipt.upload_id, ArtifactUploadId)
        assert receipt.model_dump(mode="json") == current
    for key, retired in [("uploadId","upload_id"), ("artifactId","artifact_id"),
                         ("artifactUri","artifact_uri"), ("byteLen","byte_len"),
                         ("mimeType","mime_type"), ("createdAt","created_at")]:
        for keep_current in [False, True]:
            bad = deepcopy(current)
            bad[retired] = bad[key]
            if not keep_current:
                del bad[key]
            for decode in decoders(bad):
                with pytest.raises((ValueError, TypeError)):
                    decode()
    for key, value in [("uploadId", str(uuid.uuid4())), ("uploadId", upload_id.replace("-", "")),
                       ("artifactId", str(uuid.uuid4())), ("byteLen", True),
                       ("byteLen", -1), ("byteLen", 2**64),
                       ("createdAt", 123), ("createdAt", "123"),
                       ("sha256", "a" * 64 + "\n"),
                       ("artifactUri", f"artifact://{uuid7()}")]:
        bad = deepcopy(current)
        bad[key] = value
        for decode in decoders(bad):
            with pytest.raises((ValueError, TypeError)):
                decode()


@pytest.mark.parametrize("text", [
    "2026-10-05T12:34:56.123456789Z", "2016-12-31T23:59:60.123456789Z",
    "0000-01-01T00:00:00.000000001Z", "-0001-01-01T00:00:00.000000001Z",
    "+10000-01-01T00:00:00.000000001Z", "-262143-01-01T00:00:00Z",
    "+262142-12-31T23:59:59.999999999Z", "2026-10-05T14:34:56.123456789+02:00",
])
def test_artifact_timestamp_fields_preserve_text_on_all_decoders_and_copy(text):
    import json
    from pathlib import Path
    from pydantic import TypeAdapter
    from veoveo_mcp.contract.artifacts import ArtifactUploadReceipt, ComplianceMetadata

    fixture = Path(__file__).resolve().parents[3] / "platform/artifacts/contract/tests/fixtures/metadata-output.json"
    metadata = json.loads(fixture.read_text())
    metadata["createdAt"] = text
    receipt = dict(uploadId=str(uuid7()), artifactId=metadata["artifactId"],
                   artifactUri=metadata["artifactUri"], sha256="a" * 64, byteLen=1,
                   mimeType="text/plain", filename="value.txt", createdAt=text)
    cases = [
        (ArtifactMetadata, metadata, "createdAt"),
        (ArtifactUploadReceipt, receipt, "createdAt"),
        (ComplianceMetadata, dict(retentionExpiresAt=text), "retentionExpiresAt"),
        (PutArtifactRequest, dict(retentionExpiresAt=text), "retentionExpiresAt"),
        (IssueArtifactWriteCapabilityRequest,
         dict(taskId=str(uuid7()), expiresAt=text, maxArtifactCount=1, maxTotalBytes=1), "expiresAt"),
        (IssuedArtifactWriteCapability,
         dict(capabilityId=str(uuid7()), taskId=str(uuid7()), secret="s" * 32, expiresAt=text), "expiresAt"),
    ]
    for model, wire, field in cases:
        adapter = TypeAdapter(model)
        for admitted in [model.model_validate(wire), model.model_validate_json(json.dumps(wire)),
                         adapter.validate_python(wire), adapter.validate_json(json.dumps(wire))]:
            assert admitted.model_dump(mode="json")[field] == text
            assert json.loads(admitted.model_dump_json())[field] == text
            assert admitted.model_copy().model_dump(mode="json")[field] == text
            for invalid in ["2026-02-30T00:00:00Z", True, 1, datetime.now(timezone.utc)]:
                with pytest.raises(ValueError):
                    admitted.model_copy(update={field: invalid})
            forged = object.__new__(ChronoTimestamp)
            object.__setattr__(forged, "wire", "2026-02-30T00:00:00Z")
            with pytest.raises(ValueError):
                admitted.model_copy(update={field: forged})
            # A forged owner copy must also re-admit its timestamp field.
            native = next(name for name, definition in model.model_fields.items() if definition.alias == field)
            poison = model.model_construct(**{**admitted.__dict__, native: "2026-02-30T00:00:00Z"})
            with pytest.raises(ValueError):
                poison.model_copy()
        for invalid in ["2026-02-30T00:00:00Z", True, 1, "2026-01-01T00:00:00"]:
            bad = {**wire, field: invalid}
            for decode in [lambda: model.model_validate(bad),
                           lambda: model.model_validate_json(json.dumps(bad)),
                           lambda: adapter.validate_python(bad),
                           lambda: adapter.validate_json(json.dumps(bad))]:
                with pytest.raises(ValueError):
                    decode()
    request = PutArtifactRequest(retentionExpiresAt=text)
    assert request.wire()["retentionExpiresAt"] == text
    request.retention_expires_at = "2026-02-30T00:00:00Z"
    with pytest.raises(ValueError):
        request.wire()
