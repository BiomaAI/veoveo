"""Strict simulator-hosted shared camera-stream contracts."""

from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Literal

from pydantic import ConfigDict, Field
from veoveo_mcp.contract.wire import CurrentWireModel
from pydantic.alias_generators import to_camel


class WireModel(CurrentWireModel):
    model_config = ConfigDict(
        alias_generator=to_camel,
        validate_by_alias=True,
        validate_by_name=False,
        extra="forbid",
    )


class CameraHealth(str, Enum):
    READY = "ready"
    FAILED = "failed"


class ProductLifecycle(str, Enum):
    STARTING = "starting"
    READY = "ready"
    FAILED = "failed"


class ViewLifecycle(str, Enum):
    READY = "ready"
    LIVE = "live"
    CLOSED = "closed"


class CameraDescriptor(WireModel):
    schema_version: Literal["veoveo.ai/live-view/v4"]
    session_id: str = Field(min_length=1, max_length=128)
    camera_id: str = Field(min_length=1, max_length=128)
    rig: str = "fixed"
    width_px: int = Field(ge=16, le=16_384)
    height_px: int = Field(ge=16, le=16_384)
    frame_rate_millihertz: int = Field(ge=1_000, le=240_000)
    health: CameraHealth
    revision: int = Field(ge=1)


class CameraRegion(WireModel):
    camera_id: str = Field(min_length=1, max_length=128)
    x_px: int = Field(ge=0)
    y_px: int = Field(ge=0)
    width_px: int = Field(ge=16, le=16_384)
    height_px: int = Field(ge=16, le=16_384)


class StreamProduct(WireModel):
    stream_product_id: str = Field(min_length=1, max_length=128)
    camera_regions: tuple[CameraRegion, ...] = Field(min_length=1)
    coded_width_px: int = Field(ge=16, le=16_384)
    coded_height_px: int = Field(ge=16, le=16_384)
    lifecycle: ProductLifecycle
    active_viewers: int = Field(ge=0)
    connected_viewers: int = Field(ge=0)
    nvenc_sessions: int = Field(ge=0, le=1)
    encoded_frames: int = Field(ge=0)
    source_to_render_samples: int = Field(ge=0)


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


class MediaEndpoint(WireModel):
    transport: Literal["web_socket_h264"] = "web_socket_h264"
    stream_url: str


class LiveViewState(WireModel):
    schema_version: Literal["veoveo.ai/live-view/v4"]
    live_view_id: str
    resource_uri: str
    session_id: str
    camera_id: str
    stream_product_id: str
    owner: str
    viewer_actor: str
    viewer_instance_id: str
    lifecycle: ViewLifecycle
    codec: Literal["h264"] = "h264"
    hardware_encoder: Literal["nvidia_nvenc"] = "nvidia_nvenc"
    width_px: int = Field(ge=16, le=16_384)
    height_px: int = Field(ge=16, le=16_384)
    coded_width_px: int = Field(ge=16, le=16_384)
    coded_height_px: int = Field(ge=16, le=16_384)
    source_region: CameraRegion
    frame_rate_millihertz: int = Field(ge=1_000, le=240_000)
    connected_viewers: int = Field(ge=0)
    endpoint: MediaEndpoint
    created_at: datetime
    expires_at: datetime


class LiveViewConnection(WireModel):
    stream: LiveViewState
    access_token: str = Field(min_length=43, max_length=256)


class CloseLiveViewResult(WireModel):
    resource_uri: str
    closed: bool


class GetFixtureStateRequest(WireModel):
    pass


class FixtureState(WireModel):
    schema_version: Literal["veoveo.ai/simulator-hosted-live-view-fixture/v2"]
    session_id: str
    cameras: tuple[CameraDescriptor, ...]
    stream_products: tuple[StreamProduct, ...]
