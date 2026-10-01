"""Signed request correlation from the platform audit contract."""

from ipaddress import IPv4Address, IPv6Address
from typing import Annotated, NewType

from pydantic import AfterValidator, BaseModel, BeforeValidator, ConfigDict, StringConstraints

_AuditRequestId = NewType("_AuditRequestId", str)
_AuditTraceId = NewType("_AuditTraceId", str)
_AuditSpanId = NewType("_AuditSpanId", str)


def _nonzero(value: str) -> str:
    if not value.strip("0"):
        raise ValueError("audit trace and span IDs must be nonzero")
    return value


AuditRequestId = Annotated[
    _AuditRequestId,
    StringConstraints(pattern=r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"),
]
AuditTraceId = Annotated[
    _AuditTraceId,
    StringConstraints(pattern=r"^[0-9a-f]{32}$"),
    AfterValidator(_nonzero),
]
AuditSpanId = Annotated[
    _AuditSpanId,
    StringConstraints(pattern=r"^[0-9a-f]{16}$"),
    AfterValidator(_nonzero),
]


def _ip_address(value: object) -> str | IPv4Address | IPv6Address:
    # Rust's IpAddr accepts textual addresses without IPv6 zone identifiers.
    if not isinstance(value, (str, IPv4Address, IPv6Address)) or "%" in str(value):
        raise ValueError("source IP must be an IPv4 or IPv6 address without a zone")
    return value


class AuditRequest(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    id: AuditRequestId
    trace_id: AuditTraceId
    span_id: AuditSpanId
    source_ip: Annotated[IPv4Address | IPv6Address, BeforeValidator(_ip_address)] | None = None
