"""Usage report contracts, shared with the Rust `mcp-contract` crate."""

from __future__ import annotations

from enum import Enum
from typing import Any
import math

from pydantic.alias_generators import to_camel
from .wire import CurrentWireModel
from pydantic import AwareDatetime, BaseModel, ConfigDict, model_validator


class UsageKind(str, Enum):
    ESTIMATE = "estimate"
    ACTUAL = "actual"


class UsageRecord(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid", use_enum_values=True, frozen=True)

    task_id: str
    source_id: str | None = None
    provider_job_id: str | None = None
    model_id: str
    kind: UsageKind
    quantity: float | None = None
    unit: str | None = None
    amount: float | None = None
    currency: str | None = None
    recorded_at: AwareDatetime
    metadata: Any = None


    @model_validator(mode="after")
    def _admit(self):
        for value in (self.quantity, self.amount):
            if value is not None and not math.isfinite(value):
                raise ValueError("usage quantities and amounts must be finite")
        return self

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})


class UsageReport(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid", use_enum_values=True, frozen=True)

    task_id: str
    usage_uri: str
    records: tuple[UsageRecord, ...] = ()
    total_amount: float | None = None
    currency: str | None = None
    total_kind: UsageKind | None = None

    @model_validator(mode="after")
    def _admit(self):
        if any(record.task_id != self.task_id for record in self.records):
            raise ValueError("usage record belongs to another Task")
        kinds = {UsageKind(record.kind) for record in self.records}
        kind = (UsageKind.ACTUAL if UsageKind.ACTUAL in kinds else
                UsageKind.ESTIMATE if UsageKind.ESTIMATE in kinds else None)
        selected = [record for record in self.records if UsageKind(record.kind) == kind]
        currency = _common_currency(selected)
        amount = _sum_amounts(selected, currency) if currency else None
        if amount is not None and not math.isfinite(amount):
            raise ValueError("usage total must be finite")
        if (self.total_kind, self.currency, self.total_amount) != (kind, currency, amount):
            raise ValueError("usage totals disagree with selected records")
        return self

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})

    @classmethod
    def build(
        cls, task_id: str, usage_uri: str, records: list[UsageRecord]
    ) -> "UsageReport":
        kinds = {UsageKind(record.kind) for record in records}
        if UsageKind.ACTUAL in kinds:
            total_kind = UsageKind.ACTUAL
        elif UsageKind.ESTIMATE in kinds:
            total_kind = UsageKind.ESTIMATE
        else:
            total_kind = None
        totals = [
            record for record in records if UsageKind(record.kind) == total_kind
        ]
        currency = _common_currency(totals)
        total_amount = _sum_amounts(totals, currency) if currency else None
        return cls(
            taskId=task_id,
            usageUri=usage_uri,
            records=records,
            totalAmount=total_amount,
            currency=currency,
            totalKind=total_kind,
        )

    def wire(self) -> dict[str, Any]:
        return self.model_dump(mode="json", exclude_none=True)


def _common_currency(records: list[UsageRecord]) -> str | None:
    currencies = [record.currency for record in records if record.currency]
    if not currencies:
        return None
    first = currencies[0]
    return first if all(currency == first for currency in currencies) else None


def _sum_amounts(records: list[UsageRecord], currency: str) -> float | None:
    amounts = [
        record.amount
        for record in records
        if record.currency == currency and record.amount is not None
    ]
    return sum(amounts) if amounts else None
