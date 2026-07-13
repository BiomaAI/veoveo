"""Usage report contracts, shared with the Rust `mcp-contract` crate."""

from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Any

from pydantic import BaseModel, ConfigDict


class UsageKind(str, Enum):
    ESTIMATE = "estimate"
    ACTUAL = "actual"


class UsageRecord(BaseModel):
    model_config = ConfigDict(use_enum_values=True)

    task_id: str
    source_id: str | None = None
    provider_job_id: str | None = None
    model_id: str
    kind: UsageKind
    quantity: float | None = None
    unit: str | None = None
    amount: float | None = None
    currency: str | None = None
    recorded_at: datetime
    metadata: Any = None


class UsageReport(BaseModel):
    model_config = ConfigDict(use_enum_values=True)

    task_id: str
    usage_uri: str
    records: list[UsageRecord] = []
    total_amount: float | None = None
    currency: str | None = None
    total_kind: UsageKind | None = None

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
            task_id=task_id,
            usage_uri=usage_uri,
            records=records,
            total_amount=total_amount,
            currency=currency,
            total_kind=total_kind,
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
