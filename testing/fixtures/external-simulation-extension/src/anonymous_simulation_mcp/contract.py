"""Strict simulator-hosted camera, product, and viewer-lease contracts."""

from __future__ import annotations

from datetime import datetime
from enum import Enum

from pydantic import BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel


class WireModel(BaseModel):
    model_config = ConfigDict(
        alias_generator=to_camel,
        populate_by_name=True,
        extra="forbid",
    )


class CameraHealth(str, Enum):
    READY = "ready"
    FAILED = "failed"


class ProductLifecycle(str, Enum):
    INACTIVE = "inactive"
    READY = "ready"
    FAILED = "failed"


class LeaseLifecycle(str, Enum):
    READY = "ready"
    LIVE = "live"
    CLOSED = "closed"


class CameraDescriptor(WireModel):
    schema_version: str = "veoveo.io/live-view/v2"
    session_id: str = Field(min_length=1, max_length=128)
    camera_id: str = Field(min_length=1, max_length=128)
    rig: str = "fixed"
    width_px: int = Field(ge=16, le=16_384)
    height_px: int = Field(ge=16, le=16_384)
    frame_rate_millihertz: int = Field(ge=1_000, le=240_000)
    health: CameraHealth
    revision: int = Field(ge=1)


class StreamProduct(WireModel):
    schema_version: str = "veoveo.io/live-view/v2"
    stream_product_id: str = Field(min_length=1, max_length=128)
    capacity_slot: int = Field(ge=0, le=1_023)
    camera_id: str | None = Field(default=None, min_length=1, max_length=128)
    live_view_id: str | None = Field(default=None, min_length=1, max_length=128)
    lifecycle: ProductLifecycle
    codec: str = "h264"
    hardware_encoder: str = "nvidia_nvenc"
    render_products: int = Field(ge=0, le=1)
    encoder_sessions: int = Field(ge=0, le=1)
    active_viewer_leases: int = Field(ge=0, le=1)
    connected_viewers: int = Field(ge=0)
    last_frame_sequence: int = Field(ge=0)


class ListLiveCamerasRequest(WireModel):
    session_id: str = Field(min_length=1, max_length=128)


class OpenLiveViewRequest(ListLiveCamerasRequest):
    camera_id: str = Field(min_length=1, max_length=128)
    viewer_instance_id: str = Field(min_length=8, max_length=128)


class RenewLiveViewRequest(ListLiveCamerasRequest):
    live_view_id: str = Field(min_length=1, max_length=128)
    viewer_instance_id: str = Field(min_length=8, max_length=128)


class CloseLiveViewRequest(RenewLiveViewRequest):
    pass


class ViewerLease(WireModel):
    schema_version: str = "veoveo.io/live-view/v2"
    live_view_id: str
    resource_uri: str
    session_id: str
    camera_id: str
    stream_product_id: str
    capacity_slot: int = Field(ge=0, le=1_023)
    owner: str
    viewer_actor: str
    viewer_instance_id: str
    lifecycle: LeaseLifecycle
    signaling_url: str
    media_host: str
    media_port: int
    created_at: datetime
    expires_at: datetime


class LiveViewConnection(WireModel):
    stream: ViewerLease
    access_token: str = Field(min_length=43, max_length=256)


class CloseLiveViewResult(WireModel):
    resource_uri: str
    closed: bool


class GetFixtureStateRequest(WireModel):
    pass


class FixtureState(WireModel):
    schema_version: str = "veoveo.io/simulator-hosted-live-view-fixture/v1"
    session_id: str
    cameras: tuple[CameraDescriptor, ...]
    stream_products: tuple[StreamProduct, ...]
    active_viewer_leases: int = Field(ge=0)
