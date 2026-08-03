from __future__ import annotations

import hashlib
import json
import os
import queue
import struct
import tempfile
import threading
import time
import unittest
from dataclasses import replace
from http import HTTPStatus
from io import BytesIO
from pathlib import Path
from types import ModuleType
from unittest.mock import Mock, patch

from veoveo_simulation_view.camera import (
    READINESS_RENDER_PRODUCT_NAME,
    CameraPool,
    FrameHealth,
    HydraRenderProductProbe,
    _rig_matrix,
    livestream_aov_arguments,
    render_product_name,
)
from veoveo_simulation_view.config import RendererConfig
from veoveo_simulation_view.contracts import (
    CameraBinding,
    ContractError,
    InterpolationPolicy,
    PoseSourceBinding,
    RenderViewport,
    SceneBinding,
    SessionBinding,
)
from veoveo_simulation_view.interpolation import (
    InterpolationResetReason,
    PoseInterpolator,
)
from veoveo_simulation_view.layers import LayerCatalog, StreamedWorldManager
from veoveo_simulation_view.lighting import GovernedLighting
from veoveo_simulation_view.pose import (
    EntityPose,
    PoseMirror,
    PosePollResult,
    PoseSampleKind,
    PoseSnapshot,
    decode_snapshot,
)
from veoveo_simulation_view.renderer_setup import (
    CESIUM_EXTENSION_ID,
    CESIUM_MDL_MODULE_NAME,
    CESIUM_SHOW_ON_STARTUP_SETTING,
    MATERIAL_ALLOWLIST_SETTING,
    MATERIAL_SEARCH_PATH_SETTING,
    RENDERER_MDL_SEARCH_PATH_SETTING,
    TANGENT_FRAME_SETTING,
    RendererFailureCode,
    RendererInitializationError,
    configure_headless_cesium_extension,
    disable_render_product_grid,
    ensure_cesium_material_runtime,
    suppress_interactive_cesium_viewport_updates,
    VIEWPORT_GRID_ENABLED_SETTING,
)
from veoveo_simulation_view.runtime import Renderer, SessionRuntime
from veoveo_simulation_view.scene import ArtifactMaterializer, ArtifactStore


class FakeSettings:
    def __init__(self, values: dict[str, object] | None = None) -> None:
        self.values = dict(values or {})

    def get(self, name: str) -> object | None:
        return self.values.get(name)

    def get_as_string(self, name: str) -> str:
        value = self.values.get(name, "")
        if not isinstance(value, str):
            raise TypeError(f"{name} is not a string")
        return value

    def set(self, name: str, value: object) -> None:
        self.values[name] = value

    def set_string_array(self, name: str, value: list[str]) -> None:
        self.values[name] = list(value)

    def set_string(self, name: str, value: str) -> None:
        self.values[name] = value


class RendererContractsTest(unittest.TestCase):
    def test_headless_renderer_disables_cesium_interactive_ui_before_startup(
        self,
    ) -> None:
        settings = FakeSettings({CESIUM_SHOW_ON_STARTUP_SETTING: True})

        configure_headless_cesium_extension(settings)

        self.assertIs(settings.get(CESIUM_SHOW_ON_STARTUP_SETTING), False)

    def test_headless_renderer_disables_grid_in_render_products(self) -> None:
        settings = FakeSettings({VIEWPORT_GRID_ENABLED_SETTING: True})

        disable_render_product_grid(settings)

        self.assertIs(settings.get(VIEWPORT_GRID_ENABLED_SETTING), False)

    def test_headless_renderer_detaches_only_interactive_cesium_updates(
        self,
    ) -> None:
        class CesiumOmniverseExtension:
            def __init__(self) -> None:
                self._on_update_subscription = Mock()

        class Modules:
            def __init__(self, extension: object) -> None:
                self._started_extensions = [(extension, "cesium.omniverse")]

        manager = Mock()
        manager.get_enabled_extension_id.return_value = CESIUM_EXTENSION_ID
        extension = CesiumOmniverseExtension()
        subscription = extension._on_update_subscription

        suppress_interactive_cesium_viewport_updates(
            manager,
            extension_registry={CESIUM_EXTENSION_ID: Modules(extension)},
        )

        subscription.unsubscribe.assert_called_once_with()
        self.assertIsNone(extension._on_update_subscription)

    def test_headless_renderer_rejects_unknown_cesium_extension_shape(
        self,
    ) -> None:
        manager = Mock()
        manager.get_enabled_extension_id.return_value = CESIUM_EXTENSION_ID

        with self.assertRaises(RendererInitializationError) as caught:
            suppress_interactive_cesium_viewport_updates(
                manager,
                extension_registry={},
            )

        self.assertEqual(
            caught.exception.failure.code,
            RendererFailureCode.RENDERER_INITIALIZATION_FAILED,
        )

    def test_headless_renderer_rejects_a_different_cesium_version(
        self,
    ) -> None:
        manager = Mock()
        manager.get_enabled_extension_id.return_value = "cesium.omniverse-0.30.0"

        with self.assertRaises(RendererInitializationError) as caught:
            suppress_interactive_cesium_viewport_updates(
                manager,
                extension_registry={},
            )

        self.assertIn("pinned renderer profile", str(caught.exception))

    def test_cesium_material_initialization_is_exact_and_idempotent(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            extension = Path(directory) / "cesium.omniverse"
            mdl = extension / "mdl"
            mdl.mkdir(parents=True)
            (mdl / CESIUM_MDL_MODULE_NAME).write_text(
                "export material tiles() = material();\n",
                encoding="utf-8",
            )
            settings = FakeSettings(
                {
                    MATERIAL_SEARCH_PATH_SETTING: [
                        "/opt/renderer/materials",
                        str(mdl),
                        str(mdl),
                    ],
                    MATERIAL_ALLOWLIST_SETTING: [
                        "base.mdl",
                        CESIUM_MDL_MODULE_NAME,
                    ],
                    RENDERER_MDL_SEARCH_PATH_SETTING: (
                        f"/opt/renderer/mdl;{mdl};{mdl}"
                    ),
                }
            )

            first = ensure_cesium_material_runtime(
                settings, extension_directory=extension
            )
            second = ensure_cesium_material_runtime(
                settings, extension_directory=extension
            )

            self.assertEqual(first, second)
            self.assertTrue(first.mdl_assets_ready)
            self.assertTrue(first.material_search_path_ready)
            self.assertTrue(first.material_allowlist_ready)
            self.assertTrue(first.tangent_frame_ready)
            self.assertEqual(settings.get(TANGENT_FRAME_SETTING), 1)
            self.assertEqual(
                settings.get(MATERIAL_SEARCH_PATH_SETTING),
                ["/opt/renderer/materials", str(mdl)],
            )
            self.assertEqual(
                settings.get(MATERIAL_ALLOWLIST_SETTING),
                ["base.mdl", CESIUM_MDL_MODULE_NAME],
            )
            self.assertEqual(
                settings.get(RENDERER_MDL_SEARCH_PATH_SETTING),
                f"/opt/renderer/mdl;{mdl}",
            )

    def test_cesium_material_initialization_reports_missing_assets(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(RendererInitializationError) as missing:
                ensure_cesium_material_runtime(
                    FakeSettings(),
                    extension_directory=Path(directory),
                )
        self.assertEqual(
            missing.exception.failure.code,
            RendererFailureCode.CESIUM_MDL_ASSETS_MISSING,
        )
        self.assertNotIn(directory, missing.exception.failure.message)

    def test_governed_lighting_maps_directly_to_normalized_openusd_sun(
        self,
    ) -> None:
        lighting = GovernedLighting.parse(
            {
                "intensityLux": 80_000.0,
                "colorTemperatureKelvin": 6_500,
            }
        )

        values = lighting.openusd_settings()

        self.assertEqual(values.intensity, 80_000.0)
        self.assertEqual(values.exposure, 0.0)
        self.assertTrue(values.normalize)
        self.assertTrue(values.enable_color_temperature)
        self.assertEqual(values.color_temperature_kelvin, 6_500)
        self.assertEqual(values.angle_degrees, 0.53)
        self.assertEqual(values.rotation_degrees, (-45.0, -35.0, 0.0))

    def test_scene_rejects_unsupported_color_temperature(self) -> None:
        body = json.loads(
            (
                Path(__file__).resolve().parents[2]
                / "fixtures/anonymous-scene-body.json"
            ).read_text(encoding="utf-8")
        )
        body["lighting"]["colorTemperatureKelvin"] = 10_001

        with self.assertRaisesRegex(ContractError, "colorTemperatureKelvin"):
            SceneBinding.parse({"body": body, "digest": f"sha256:{'1' * 64}"})

    def test_scene_preserves_and_validates_interpolation_policy(self) -> None:
        body = json.loads(
            (
                Path(__file__).resolve().parents[2]
                / "fixtures/anonymous-scene-body.json"
            ).read_text(encoding="utf-8")
        )
        body["quality"]["interpolation"] = "linear"

        binding = SceneBinding.parse({"body": body, "digest": f"sha256:{'1' * 64}"})

        self.assertEqual(binding.interpolation, InterpolationPolicy.LINEAR)
        body["quality"]["interpolation"] = "cubic"
        with self.assertRaisesRegex(ContractError, "interpolation"):
            SceneBinding.parse({"body": body, "digest": f"sha256:{'1' * 64}"})

    def test_readiness_product_is_not_a_streamed_media_slot(self) -> None:
        config = Mock(
            maximum_render_slots=4,
            signaling_port_base=49100,
            media_port_base=47998,
            public_media_ip="192.0.2.42",
            stream_target_fps=30,
        )

        arguments = livestream_aov_arguments(config)

        self.assertEqual(len(arguments), 4 * 7)
        self.assertEqual(
            sum(argument.endswith("/targetFps=30") for argument in arguments), 4
        )
        self.assertNotIn(READINESS_RENDER_PRODUCT_NAME, " ".join(arguments))
        self.assertNotIn(
            READINESS_RENDER_PRODUCT_NAME,
            {render_product_name(slot) for slot in range(4)},
        )

    def test_render_product_is_reconfigured_without_recreation(self) -> None:
        class FakeHydraTexture:
            def __init__(self) -> None:
                self.camera_path = "/old-camera"
                self.width = 320
                self.height = 180
                self.update_history: list[bool] = []

            @property
            def updates_enabled(self) -> bool:
                return self.update_history[-1]

            @updates_enabled.setter
            def updates_enabled(self, value: bool) -> None:
                self.update_history.append(value)

            def get_settings_path(self) -> str:
                return "/hydra/slot/"

        texture = FakeHydraTexture()
        probe = HydraRenderProductProbe.__new__(HydraRenderProductProbe)
        probe._width = 320
        probe._height = 180
        probe._lock = threading.Lock()
        probe._capture_pending = True
        probe._last_capture_requested = 4.0
        probe._closed = False
        probe._generation = 3
        probe._health = None
        probe._failure = RuntimeError("stale")
        probe._subscription = object()
        probe._hydra_texture = texture

        settings = Mock()
        carb = ModuleType("carb")
        carb.__path__ = []
        carb_settings = ModuleType("carb.settings")
        carb_settings.get_settings = lambda: settings
        carb.settings = carb_settings
        with patch.dict(
            "sys.modules",
            {"carb": carb, "carb.settings": carb_settings},
        ):
            probe.reconfigure(
                camera_path="/World/SimulationView/Cameras/slot_2",
                width=640,
                height=360,
                fps=30,
            )

        self.assertIs(probe._hydra_texture, texture)
        self.assertEqual(texture.camera_path, "/World/SimulationView/Cameras/slot_2")
        self.assertEqual((texture.width, texture.height), (640, 360))
        self.assertEqual(texture.update_history, [False, True])
        self.assertEqual(probe._generation, 4)
        self.assertFalse(probe._closed)
        self.assertIsNone(probe._failure)
        settings.set.assert_called_once_with("/hydra/slot/hydraTickRate", 30)

    def test_drawable_refreshes_freshness_between_visibility_readbacks(
        self,
    ) -> None:
        resource = object()
        texture = Mock()
        texture.get_aov_info.return_value = [{"texture": {"rp_resource": resource}}]
        texture.get_frame_info.return_value = {
            "view": [float(value) for value in range(16)],
            "projection": [float(value + 16) for value in range(16)],
            "resolution": (1280, 720),
        }
        capture = Mock()
        probe = HydraRenderProductProbe.__new__(HydraRenderProductProbe)
        probe._lock = threading.Lock()
        probe._capture_pending = False
        probe._last_capture_requested = 9.75
        probe._closed = False
        probe._generation = 3
        probe._health = FrameHealth(
            sequence=7,
            observed_at=1.0,
            observed_at_iso="2026-08-03T03:00:00Z",
            visible=True,
        )
        probe._last_drawable_at = 1.0
        probe._last_drawable_at_iso = "2026-08-03T03:00:00Z"
        probe._viewport = None
        probe._failure = None
        probe._hydra_texture = texture
        probe._capture = capture

        with (
            patch(
                "veoveo_simulation_view.camera.time.monotonic",
                return_value=10.0,
            ),
            patch(
                "veoveo_simulation_view.camera._timestamp",
                return_value="2026-08-03T03:00:09Z",
            ),
        ):
            probe._on_drawable({"result_handle": 42})

        health = probe.health
        self.assertIsNotNone(health)
        self.assertEqual(health.sequence, 7)
        self.assertEqual(health.observed_at, 10.0)
        self.assertEqual(health.observed_at_iso, "2026-08-03T03:00:09Z")
        self.assertTrue(health.visible)
        self.assertEqual(
            probe.viewport,
            RenderViewport(
                view=tuple(float(value) for value in range(16)),
                projection=tuple(float(value + 16) for value in range(16)),
                width=1280,
                height=720,
            ),
        )
        capture.capture_next_frame_rp_resource_callback.assert_not_called()

    def test_streamed_world_submits_all_render_product_viewports_together(
        self,
    ) -> None:
        class FakeMatrix4d:
            def __init__(self, *values: float) -> None:
                self.values = tuple(values)

        class FakeCesiumViewport:
            def __init__(self) -> None:
                self._view_matrix: FakeMatrix4d | None = None
                self._projection_matrix: FakeMatrix4d | None = None

            @property
            def viewMatrix(self) -> FakeMatrix4d | None:
                return self._view_matrix

            @viewMatrix.setter
            def viewMatrix(self, value: FakeMatrix4d) -> None:
                if not isinstance(value, FakeMatrix4d):
                    raise TypeError("viewMatrix requires Matrix4d")
                self._view_matrix = value

            @property
            def projMatrix(self) -> FakeMatrix4d | None:
                return self._projection_matrix

            @projMatrix.setter
            def projMatrix(self, value: FakeMatrix4d) -> None:
                if not isinstance(value, FakeMatrix4d):
                    raise TypeError("projMatrix requires Matrix4d")
                self._projection_matrix = value

        class FakeStatistics:
            tileset_cached_bytes = 1024
            tiles_rendered = 8
            tiles_loading_worker = 1
            tiles_loading_main = 1

        class FakeInterface:
            def __init__(self) -> None:
                self.viewports: list[object] = []

            def on_update_frame(self, viewports: list[object], wait: bool) -> None:
                self_outer.assertIs(wait, False)
                self.viewports = viewports

            def get_render_statistics(self) -> FakeStatistics:
                return FakeStatistics()

        manager = StreamedWorldManager.__new__(StreamedWorldManager)
        manager._interface = FakeInterface()
        manager._sessions = {
            "session-1": {
                "layer": Mock(
                    budgets={
                        "maximumCacheBytes": 4096,
                        "maximumVisibleTiles": 16,
                        "maximumPendingTiles": 4,
                    }
                ),
                "tilesetPath": "/layer/Tileset",
                "startedAt": 0.0,
            }
        }
        viewports = (
            RenderViewport(
                tuple(float(value) for value in range(16)),
                tuple(float(value) for value in range(16, 32)),
                1280,
                720,
            ),
            RenderViewport(
                tuple(float(value) for value in range(32, 48)),
                tuple(float(value) for value in range(48, 64)),
                640,
                360,
            ),
        )
        bindings = ModuleType("cesium.omniverse.bindings")
        bindings.Viewport = FakeCesiumViewport
        schemas = ModuleType("cesium.usd.plugins.CesiumUsdSchemas")
        schemas.Tileset = object
        cesium = ModuleType("cesium")
        cesium.__path__ = []
        cesium_omniverse = ModuleType("cesium.omniverse")
        cesium_omniverse.__path__ = []
        cesium_usd = ModuleType("cesium.usd")
        cesium_usd.__path__ = []
        cesium_plugins = ModuleType("cesium.usd.plugins")
        cesium_plugins.__path__ = []
        pxr = ModuleType("pxr")
        gf = ModuleType("pxr.Gf")
        gf.Matrix4d = FakeMatrix4d
        pxr.Gf = gf
        self_outer = self

        with patch.dict(
            "sys.modules",
            {
                "cesium": cesium,
                "cesium.omniverse": cesium_omniverse,
                "cesium.omniverse.bindings": bindings,
                "cesium.usd": cesium_usd,
                "cesium.usd.plugins": cesium_plugins,
                "cesium.usd.plugins.CesiumUsdSchemas": schemas,
                "pxr": pxr,
                "pxr.Gf": gf,
            },
        ):
            manager.tick(viewports)
            manager._on_update_frame(None)

        self.assertEqual(len(manager._interface.viewports), 2)
        first, second = manager._interface.viewports
        self.assertEqual(first.viewMatrix.values, viewports[0].view)
        self.assertEqual(first.projMatrix.values, viewports[0].projection)
        self.assertEqual((first.width, first.height), (1280.0, 720.0))
        self.assertEqual(second.viewMatrix.values, viewports[1].view)
        self.assertEqual(second.projMatrix.values, viewports[1].projection)
        self.assertEqual((second.width, second.height), (640.0, 360.0))
        self.assertEqual(manager._sessions["session-1"]["lifecycle"], "ready")

    def test_streamed_world_registers_provider_after_first_drawable_viewport(
        self,
    ) -> None:
        class FakeMatrix4d:
            def __init__(self, *values: float) -> None:
                self.values = tuple(values)

        class FakeCesiumViewport:
            pass

        class FakeAttribute:
            def __init__(self) -> None:
                self.values: list[bool] = []

            def Set(self, value: bool) -> None:
                self.values.append(value)

        suspend = FakeAttribute()

        class FakeTileset:
            def GetSuspendUpdateAttr(self) -> FakeAttribute:
                return suspend

        class Tileset:
            @staticmethod
            def Get(stage: object, path: str) -> FakeTileset:
                self.assertIs(stage, manager._stage)
                self.assertEqual(path, "/layer/Tileset")
                return FakeTileset()

        class FakeStatistics:
            tileset_cached_bytes = 1024
            tiles_rendered = 1
            tiles_loading_worker = 0
            tiles_loading_main = 0

        self_outer = self

        class FakeInterface:
            def __init__(self) -> None:
                self.stage_changes: list[int] = []

            def on_stage_change(self, stage_id: int) -> None:
                self.stage_changes.append(stage_id)

            def on_update_frame(self, viewports: list[object], wait: bool) -> None:
                self_outer.assertTrue(viewports)
                self_outer.assertIs(wait, False)

            def get_render_statistics(self) -> FakeStatistics:
                return FakeStatistics()

        manager = StreamedWorldManager.__new__(StreamedWorldManager)
        manager._stage = object()
        manager._interface = FakeInterface()
        manager._sessions = {
            "session-1": {
                "layer": Mock(
                    budgets={
                        "maximumCacheBytes": 4096,
                        "maximumVisibleTiles": 16,
                        "maximumPendingTiles": 4,
                    }
                ),
                "tilesetPath": "/layer/Tileset",
                "providerRegistered": False,
                "coverageStartedAt": None,
                "lifecycle": "loading",
                "failure": None,
            }
        }
        viewports = (
            RenderViewport(
                tuple(float(value) for value in range(16)),
                tuple(float(value) for value in range(16, 32)),
                1280,
                720,
            ),
        )
        bindings = ModuleType("cesium.omniverse.bindings")
        bindings.Viewport = FakeCesiumViewport
        schemas = ModuleType("cesium.usd.plugins.CesiumUsdSchemas")
        schemas.Tileset = Tileset
        cesium = ModuleType("cesium")
        cesium.__path__ = []
        cesium_omniverse = ModuleType("cesium.omniverse")
        cesium_omniverse.__path__ = []
        cesium_usd = ModuleType("cesium.usd")
        cesium_usd.__path__ = []
        cesium_plugins = ModuleType("cesium.usd.plugins")
        cesium_plugins.__path__ = []
        pxr = ModuleType("pxr")
        gf = ModuleType("pxr.Gf")
        gf.Matrix4d = FakeMatrix4d
        pxr.Gf = gf
        omni = ModuleType("omni")
        omni.__path__ = []
        omni_usd = ModuleType("omni.usd")
        omni_usd.get_context = lambda: Mock(get_stage_id=lambda: 73)
        omni.usd = omni_usd

        with patch.dict(
            "sys.modules",
            {
                "cesium": cesium,
                "cesium.omniverse": cesium_omniverse,
                "cesium.omniverse.bindings": bindings,
                "cesium.usd": cesium_usd,
                "cesium.usd.plugins": cesium_plugins,
                "cesium.usd.plugins.CesiumUsdSchemas": schemas,
                "omni": omni,
                "omni.usd": omni_usd,
                "pxr": pxr,
                "pxr.Gf": gf,
            },
        ):
            manager._update_provider(viewports)

        self.assertEqual(manager._interface.stage_changes, [0, 73])
        self.assertEqual(suspend.values, [False])
        self.assertTrue(manager._sessions["session-1"]["providerRegistered"])
        self.assertIsNotNone(
            manager._sessions["session-1"]["coverageStartedAt"]
        )
        self.assertEqual(manager._sessions["session-1"]["lifecycle"], "ready")

    def test_unavailable_coverage_remains_eligible_for_recovery(self) -> None:
        class FakeMatrix4d:
            def __init__(self, *values: float) -> None:
                self.values = tuple(values)

        class FakeCesiumViewport:
            pass

        class ForbiddenAttribute:
            def Set(self, value: bool) -> None:
                raise AssertionError(
                    f"unavailable coverage must not suspend the tileset: {value}"
                )

        class FakeTileset:
            def GetSuspendUpdateAttr(self) -> ForbiddenAttribute:
                return ForbiddenAttribute()

        class Tileset:
            @staticmethod
            def Get(stage: object, path: str) -> FakeTileset:
                return FakeTileset()

        class FakeStatistics:
            tileset_cached_bytes = 0
            tiles_rendered = 0
            tiles_loading_worker = 0
            tiles_loading_main = 0

        manager = StreamedWorldManager.__new__(StreamedWorldManager)
        manager._interface = Mock()
        manager._interface.get_render_statistics.return_value = FakeStatistics()
        manager._author = Mock()
        manager._sessions = {
            "session-1": {
                "sessionId": "session-1",
                "layer": Mock(
                    budgets={
                        "maximumCacheBytes": 4096,
                        "maximumVisibleTiles": 16,
                        "maximumPendingTiles": 4,
                    }
                ),
                "tilesetPath": "/layer/Tileset",
                "providerRegistered": True,
                "coverageStartedAt": time.monotonic() - 121.0,
                "lifecycle": "loading",
                "failure": None,
            }
        }
        bindings = ModuleType("cesium.omniverse.bindings")
        bindings.Viewport = FakeCesiumViewport
        schemas = ModuleType("cesium.usd.plugins.CesiumUsdSchemas")
        schemas.Tileset = Tileset
        cesium = ModuleType("cesium")
        cesium.__path__ = []
        cesium_omniverse = ModuleType("cesium.omniverse")
        cesium_omniverse.__path__ = []
        cesium_usd = ModuleType("cesium.usd")
        cesium_usd.__path__ = []
        cesium_plugins = ModuleType("cesium.usd.plugins")
        cesium_plugins.__path__ = []
        pxr = ModuleType("pxr")
        gf = ModuleType("pxr.Gf")
        gf.Matrix4d = FakeMatrix4d
        pxr.Gf = gf
        viewport = RenderViewport(
            tuple(float(value) for value in range(16)),
            tuple(float(value) for value in range(16, 32)),
            1280,
            720,
        )

        with patch.dict(
            "sys.modules",
            {
                "cesium": cesium,
                "cesium.omniverse": cesium_omniverse,
                "cesium.omniverse.bindings": bindings,
                "cesium.usd": cesium_usd,
                "cesium.usd.plugins": cesium_plugins,
                "cesium.usd.plugins.CesiumUsdSchemas": schemas,
                "pxr": pxr,
                "pxr.Gf": gf,
            },
        ):
            manager._update_provider((viewport,))

        self.assertEqual(manager._sessions["session-1"]["lifecycle"], "failed")
        self.assertEqual(
            manager._sessions["session-1"]["failure"]["code"],
            "unavailable_coverage",
        )

    def test_streamed_world_converts_binding_failures_to_typed_health(
        self,
    ) -> None:
        manager = StreamedWorldManager.__new__(StreamedWorldManager)
        manager._interface = Mock()
        manager._sessions = {
            "session-1": {
                "layer": Mock(),
                "lifecycle": "loading",
                "failure": None,
            }
        }

        manager._update_provider(
            (
                RenderViewport(
                    tuple(float(value) for value in range(16)),
                    tuple(float(value) for value in range(16, 32)),
                    1280,
                    720,
                ),
            )
        )

        self.assertEqual(manager._sessions["session-1"]["lifecycle"], "failed")
        self.assertEqual(
            manager._sessions["session-1"]["failure"]["code"],
            "provider_unavailable",
        )
        manager._interface.on_update_frame.assert_not_called()

    def test_streamed_world_clears_provider_viewports_when_none_are_drawable(
        self,
    ) -> None:
        class FakeStatistics:
            tileset_cached_bytes = 0
            tiles_rendered = 0
            tiles_loading_worker = 0
            tiles_loading_main = 0

        manager = StreamedWorldManager.__new__(StreamedWorldManager)
        manager._interface = Mock()
        manager._interface.get_render_statistics.return_value = FakeStatistics()
        manager._author = Mock()
        manager._sessions = {
            "session-1": {
                "sessionId": "session-1",
                "layer": Mock(
                    budgets={
                        "maximumCacheBytes": 4096,
                        "maximumVisibleTiles": 16,
                        "maximumPendingTiles": 4,
                    }
                ),
                "tilesetPath": "/layer/Tileset",
                "startedAt": time.monotonic(),
                "providerAuthored": False,
                "lifecycle": "ready",
                "failure": None,
            }
        }
        bindings = ModuleType("cesium.omniverse.bindings")
        bindings.Viewport = object
        schemas = ModuleType("cesium.usd.plugins.CesiumUsdSchemas")
        schemas.Tileset = object
        cesium = ModuleType("cesium")
        cesium.__path__ = []
        cesium_omniverse = ModuleType("cesium.omniverse")
        cesium_omniverse.__path__ = []
        cesium_usd = ModuleType("cesium.usd")
        cesium_usd.__path__ = []
        cesium_plugins = ModuleType("cesium.usd.plugins")
        cesium_plugins.__path__ = []
        pxr = ModuleType("pxr")
        pxr.Gf = ModuleType("pxr.Gf")

        with patch.dict(
            "sys.modules",
            {
                "cesium": cesium,
                "cesium.omniverse": cesium_omniverse,
                "cesium.omniverse.bindings": bindings,
                "cesium.usd": cesium_usd,
                "cesium.usd.plugins": cesium_plugins,
                "cesium.usd.plugins.CesiumUsdSchemas": schemas,
                "pxr": pxr,
            },
        ):
            manager._update_provider(())

        manager._interface.on_update_frame.assert_called_once_with([], False)
        manager._author.assert_not_called()
        self.assertEqual(manager._sessions["session-1"]["lifecycle"], "loading")

    def test_mounted_camera_composes_mount_in_entity_local_space(
        self,
    ) -> None:
        class Matrix:
            def __init__(self, label: str) -> None:
                self.label = label

            def __mul__(self, other: object) -> "Matrix":
                if not isinstance(other, Matrix):
                    return NotImplemented
                return Matrix(f"{self.label} * {other.label}")

        pxr = ModuleType("pxr")
        gf = ModuleType("pxr.Gf")
        pxr.Gf = gf
        rig = {
            "kind": "mounted_entity",
            "targetEntity": "aircraft-1",
            "mount": {
                "translationM": {"x": 8.0, "y": 0.0, "z": 3.0},
                "orientationXyzw": {
                    "x": 0.5,
                    "y": -0.5,
                    "z": -0.5,
                    "w": 0.5,
                },
                "scale": {"x": 1.0, "y": 1.0, "z": 1.0},
            },
        }
        entity = Mock(
            position_enu_m=(10.0, 20.0, 30.0),
            orientation_xyzw=(0.0, 0.0, 0.0, 1.0),
        )

        with (
            patch.dict("sys.modules", {"pxr": pxr, "pxr.Gf": gf}),
            patch(
                "veoveo_simulation_view.camera._pose_matrix",
                return_value=Matrix("entity"),
            ),
            patch(
                "veoveo_simulation_view.camera._transform_matrix",
                return_value=Matrix("mount"),
            ),
        ):
            matrix, eye = _rig_matrix(rig, {"aircraft-1": entity}, previous_eye=None)

        self.assertEqual(matrix.label, "mount * entity")
        self.assertIsNone(eye)

    def test_idle_physical_slot_is_reused_by_the_next_logical_camera(
        self,
    ) -> None:
        class FakeRuntime:
            def __init__(self, binding: CameraBinding) -> None:
                self.binding = binding
                self.probe = Mock()
                self.smoothed_eye = (1.0, 2.0, 3.0)
                self.last_update = 4.0
                self.last_pose_sequence = 5
                self.pose_stale = True

            def status(self) -> dict[str, object]:
                return {"cameraId": self.binding.camera_id}

        first = CameraBinding(
            session_id="session-1",
            camera_id="camera-1",
            revision=1,
            render_slot=2,
            definition={},
        )
        second = CameraBinding(
            session_id="session-2",
            camera_id="camera-2",
            revision=1,
            render_slot=2,
            definition={},
        )
        runtime = FakeRuntime(first)
        pool = CameraPool.__new__(CameraPool)
        pool._cameras = {first.camera_id: runtime}
        pool._slots = {first.render_slot: first.camera_id}
        pool._idle = {}
        pool._probe = None

        pool.close(first.camera_id)
        runtime.probe.pause.assert_called_once_with()
        self.assertIs(pool._idle[first.render_slot], runtime)

        def configure(reused: FakeRuntime, binding: CameraBinding) -> None:
            reused.binding = binding

        with (
            patch.object(
                pool,
                "_configure_camera",
                side_effect=configure,
            ) as configure_camera,
            patch.object(pool, "_create_camera") as create_camera,
        ):
            status = pool.upsert(second)

        configure_camera.assert_called_once_with(runtime, second)
        create_camera.assert_not_called()
        self.assertEqual(status, {"cameraId": "camera-2"})
        self.assertIs(pool._cameras[second.camera_id], runtime)

    def test_identical_camera_upsert_does_not_reconfigure_rtx_product(
        self,
    ) -> None:
        binding = CameraBinding(
            session_id="session-1",
            camera_id="camera-1",
            revision=1,
            render_slot=2,
            definition={"widthPx": 1280, "heightPx": 720},
        )
        runtime = Mock(binding=binding)
        runtime.status.return_value = {
            "cameraId": binding.camera_id,
            "ready": True,
        }
        pool = CameraPool.__new__(CameraPool)
        pool._cameras = {binding.camera_id: runtime}
        pool._slots = {binding.render_slot: binding.camera_id}
        pool._idle = {}

        with patch.object(pool, "_configure_camera") as configure_camera:
            status = pool.upsert(binding)

        configure_camera.assert_not_called()
        self.assertEqual(status, {"cameraId": binding.camera_id, "ready": True})

    def test_slot_zero_is_idle_without_reconfiguring_readiness_probe(
        self,
    ) -> None:
        class FakeRuntime:
            def __init__(self, binding: CameraBinding) -> None:
                self.binding = binding
                self.probe = Mock()
                self.smoothed_eye = None
                self.last_update = 0.0
                self.last_pose_sequence = None
                self.pose_stale = False

        binding = CameraBinding(
            session_id="session-1",
            camera_id="camera-1",
            revision=1,
            render_slot=0,
            definition={},
        )
        readiness_probe = object()
        runtime = FakeRuntime(binding)
        pool = CameraPool.__new__(CameraPool)
        pool._cameras = {binding.camera_id: runtime}
        pool._slots = {0: binding.camera_id}
        pool._idle = {}
        pool._probe = readiness_probe

        pool.close(binding.camera_id)

        runtime.probe.pause.assert_called_once_with()
        self.assertIs(pool._idle[0], runtime)
        self.assertIs(pool._probe, readiness_probe)
        self.assertEqual(pool._cameras, {})
        self.assertEqual(pool._slots, {})

    def test_config_requires_disjoint_bounded_port_ranges(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            catalog = Path(directory) / "layers.json"
            catalog.write_text(
                json.dumps(
                    {
                        "schemaVersion": "veoveo.io/simulation-view-layer-catalog/v1",
                        "layers": [],
                    }
                ),
                encoding="utf-8",
            )
            values = {
                "SIMULATION_VIEW_RENDERER_CONTROL_TOKEN": "a" * 32,
                "SIMULATION_VIEW_PUBLIC_MEDIA_IP": "192.0.2.42",
                "SIMULATION_VIEW_ARTIFACT_DIRECTORY": f"{directory}/artifacts",
                "SIMULATION_VIEW_POSE_DIRECTORY": f"{directory}/pose",
                "SIMULATION_VIEW_RENDERER_CACHE_DIRECTORY": f"{directory}/cache",
                "SIMULATION_VIEW_MAXIMUM_RENDER_SLOTS": "4",
                "SIMULATION_VIEW_SIGNALING_PORT_BASE": "49100",
                "SIMULATION_VIEW_MEDIA_PORT_BASE": "47998",
                "SIMULATION_VIEW_STREAM_TARGET_FPS": "15",
                "SIMULATION_VIEW_LAYER_CATALOG": str(catalog),
            }
            with patch.dict(os.environ, values, clear=True):
                config = RendererConfig.from_environment()
            self.assertEqual(config.signaling_port_base + 3, 49103)
            self.assertEqual(config.media_port_base + 3, 48001)
            self.assertEqual(config.stream_target_fps, 15)
            self.assertEqual(config.maximum_artifact_bytes, 4 * 1024 * 1024 * 1024)

    def test_layer_catalog_requires_secret_without_exposing_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layers.json"
            value = {
                "schemaVersion": "veoveo.io/simulation-view-layer-catalog/v1",
                "layers": [
                    {
                        "layerId": "installation-world",
                        "layerType": "streamed_3d_tiles",
                        "source": {
                            "kind": "cesium_ion",
                            "assetId": 1,
                            "serverUrl": "https://tiles.example/",
                            "apiUrl": "https://api.example/",
                            "applicationId": 2,
                            "credentialEnvironment": "SIMULATION_VIEW_LAYER_TOKEN",
                        },
                        "allowedHosts": ["tiles.example", "api.example"],
                        "allowedRedirectHosts": ["assets.example"],
                        "budgets": {
                            "maximumCacheBytes": 1024,
                            "maximumTileBytes": 512,
                            "maximumVisibleTiles": 64,
                            "maximumPendingTiles": 8,
                            "maximumScreenSpaceError": 16.0,
                        },
                        "license": {
                            "identifier": "provider-terms",
                            "attribution": "Installation imagery",
                            "attributionUrl": "https://example.com/terms",
                            "displayRequired": True,
                        },
                        "georeference": {
                            "world": "frames://world/demo/revision/r1",
                            "frameRevision": {
                                "uri": "frames://world/demo/revision/r1",
                                "digest": f"sha256:{'1' * 64}",
                            },
                            "localEnuFrame": (
                                "frames://world/demo/revision/r1/frame/simulation"
                            ),
                            "origin": {
                                "latitudeDegrees": 40.0,
                                "longitudeDegrees": -105.0,
                                "ellipsoidHeightM": 1600.0,
                            },
                        },
                    }
                ],
            }
            path.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(
                ValueError, "SIMULATION_VIEW_LAYER_TOKEN"
            ) as missing:
                LayerCatalog.load(path, {})
            self.assertNotIn("browser-safe-secret", str(missing.exception))

            catalog = LayerCatalog.load(
                path, {"SIMULATION_VIEW_LAYER_TOKEN": "browser-safe-secret"}
            )
            layer = catalog.get("installation-world")
            self.assertEqual(layer.credential, "browser-safe-secret")
            self.assertNotIn("browser-safe-secret", repr(layer))

    def test_streamed_world_close_quiesces_before_removal_without_token_reload(
        self,
    ) -> None:
        events: list[str] = []

        class FakeAttribute:
            def Set(self, value: object) -> None:
                self_outer.assertIs(value, True)
                events.append("suspend")

        class ForbiddenTokenAttribute:
            def Clear(self) -> None:
                raise AssertionError("close must not reload by clearing the token")

        class FakePrim:
            def IsValid(self) -> bool:
                return True

        class FakeTileset:
            def GetPrim(self) -> FakePrim:
                return FakePrim()

            def GetSuspendUpdateAttr(self) -> FakeAttribute:
                return FakeAttribute()

            def GetIonAccessTokenAttr(self) -> ForbiddenTokenAttribute:
                return ForbiddenTokenAttribute()

        class Tileset:
            @staticmethod
            def Get(stage: object, path: str) -> FakeTileset:
                self_outer.assertIs(stage, manager._stage)
                self_outer.assertEqual(path, "/layer/Tileset")
                return FakeTileset()

        class FakeInterface:
            def on_update_frame(self, viewports: list[object], wait: bool) -> None:
                self_outer.assertEqual(viewports, [])
                self_outer.assertIs(wait, False)
                events.append("provider_update")

        class FakeStage:
            def __init__(self) -> None:
                self.target = "original"

            def GetEditTarget(self) -> str:
                return self.target

            def GetSessionLayer(self) -> str:
                return "session-layer"

            def SetEditTarget(self, target: object) -> None:
                self.target = target

            def RemovePrim(self, path: str) -> bool:
                self_outer.assertEqual(path, "/layer")
                events.append("remove")
                return True

        class Usd:
            @staticmethod
            def EditTarget(layer: object) -> object:
                return layer

        self_outer = self
        manager = StreamedWorldManager.__new__(StreamedWorldManager)
        manager._stage = FakeStage()
        manager._interface = FakeInterface()
        manager._sessions = {
            "session-1": {
                "rootPath": "/layer",
                "tilesetPath": "/layer/Tileset",
            }
        }
        schemas = ModuleType("cesium.usd.plugins.CesiumUsdSchemas")
        schemas.Tileset = Tileset
        cesium = ModuleType("cesium")
        cesium.__path__ = []
        cesium_usd = ModuleType("cesium.usd")
        cesium_usd.__path__ = []
        cesium_plugins = ModuleType("cesium.usd.plugins")
        cesium_plugins.__path__ = []
        pxr = ModuleType("pxr")
        pxr.Usd = Usd

        with patch.dict(
            "sys.modules",
            {
                "cesium": cesium,
                "cesium.usd": cesium_usd,
                "cesium.usd.plugins": cesium_plugins,
                "cesium.usd.plugins.CesiumUsdSchemas": schemas,
                "pxr": pxr,
            },
        ):
            manager.close("session-1")

        self.assertEqual(
            events,
            ["suspend", "provider_update", "remove", "provider_update"],
        )
        self.assertEqual(manager._sessions, {})
        self.assertEqual(manager._stage.target, "original")

    def test_renderer_retains_session_when_provider_teardown_fails(
        self,
    ) -> None:
        binding = SessionBinding.parse({"sessionId": "session-1", "epochId": "epoch-1"})
        renderer = Renderer.__new__(Renderer)
        renderer._sessions = {binding.session_id: SessionRuntime(binding=binding)}
        renderer._cameras = Mock()
        renderer._layers = Mock()
        renderer._layers.close.side_effect = ContractError(
            "streamed-world provider teardown failed"
        )
        renderer._scenes = Mock()
        renderer._streams = {}
        command = Mock(
            operation="delete_session",
            session_id=binding.session_id,
        )

        with self.assertRaisesRegex(ContractError, "provider teardown failed"):
            renderer._execute(command)

        self.assertIn(binding.session_id, renderer._sessions)
        renderer._cameras.close_session.assert_called_once_with(binding.session_id)
        renderer._scenes.close.assert_not_called()

    def test_renderer_retries_a_partially_completed_session_teardown(
        self,
    ) -> None:
        binding = SessionBinding.parse({"sessionId": "session-1", "epochId": "epoch-1"})
        renderer = Renderer.__new__(Renderer)
        renderer._sessions = {binding.session_id: SessionRuntime(binding=binding)}
        renderer._cameras = Mock()
        renderer._layers = Mock()
        renderer._layers.close.side_effect = [
            ContractError("streamed-world provider teardown failed"),
            None,
        ]
        renderer._scenes = Mock()
        renderer._streams = {}
        command = Mock(
            operation="delete_session",
            session_id=binding.session_id,
        )

        with self.assertRaises(ContractError):
            renderer._execute(command)
        result = renderer._execute(command)

        self.assertEqual(result.status, HTTPStatus.NO_CONTENT)
        self.assertNotIn(binding.session_id, renderer._sessions)
        self.assertEqual(renderer._cameras.close_session.call_count, 2)
        self.assertEqual(renderer._layers.close.call_count, 2)
        renderer._scenes.close.assert_called_once_with(binding.session_id)

    def test_identical_scene_and_pose_puts_do_not_mutate_native_runtime(
        self,
    ) -> None:
        binding = SessionBinding.parse({"sessionId": "session-1", "epochId": "epoch-1"})
        scene_body = json.loads(
            (
                Path(__file__).resolve().parents[2]
                / "fixtures/anonymous-scene-body.json"
            ).read_text(encoding="utf-8")
        )
        scene_body["sessionId"] = binding.session_id
        scene_body["epochId"] = binding.epoch_id
        scene = SceneBinding.parse({"body": scene_body, "digest": f"sha256:{'1' * 64}"})
        pose = PoseSourceBinding(
            session_id=binding.session_id,
            epoch_id=binding.epoch_id,
            frame_uri=scene.frame_uri,
            frame_digest=scene.frame_digest,
            entity_table_revision=1,
            entity_table_digest=f"sha256:{'2' * 64}",
            maximum_entities=20,
            maximum_message_bytes=4 * 1024 * 1024,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            producer_id="producer-1",
            producer_spiffe_id="spiffe://example.test/producer-1",
            authorization_revision=1,
            expires_at="2026-08-03T03:00:00Z",
            revoked=False,
        )
        mirror = Mock()
        renderer = Renderer.__new__(Renderer)
        renderer._sessions = {
            binding.session_id: SessionRuntime(
                binding=binding,
                scene=scene,
                pose=mirror,
                pose_binding=pose,
            )
        }
        renderer._layers = Mock()
        renderer._scenes = Mock()

        scene_result = renderer._execute(
            Mock(
                operation="put_scene",
                session_id=binding.session_id,
                value=scene,
            )
        )
        pose_result = renderer._execute(
            Mock(
                operation="put_pose_source",
                session_id=binding.session_id,
                value=pose,
            )
        )

        self.assertEqual(scene_result.status, 200)
        self.assertEqual(pose_result.status, 200)
        renderer._layers.bind.assert_not_called()
        renderer._scenes.bind.assert_not_called()
        mirror.renew.assert_not_called()

    def test_scene_and_all_cameras_consume_the_same_rendered_pose_frame(
        self,
    ) -> None:
        class Clock:
            now = 0

            def __call__(self) -> int:
                return self.now

        def source(sequence: int, timestamp: int, x: float) -> PoseSnapshot:
            return PoseSnapshot(
                session_id="session-1",
                epoch_id="epoch-1",
                sequence=sequence,
                simulation_timestamp_ns=timestamp,
                frame_uri="frames://world/synthetic/revision/r1",
                frame_digest=f"sha256:{'1' * 64}",
                entity_table_revision=1,
                entity_table_digest=f"sha256:{'2' * 64}",
                entities=(
                    EntityPose(
                        entity_id="entity-1",
                        position_enu_m=(x, 0.0, 0.0),
                        orientation_xyzw=(0.0, 0.0, 0.0, 1.0),
                        active=True,
                        visible=True,
                    ),
                ),
            )

        first = source(1, 0, 0.0)
        second = source(2, 50_000_000, 10.0)
        pose = Mock(
            latest=second,
            stale=False,
        )
        pose.poll.side_effect = [
            PosePollResult(PoseSampleKind.ACCEPTED, first),
            PosePollResult(PoseSampleKind.ACCEPTED, second),
            None,
        ]
        clock = Clock()
        interpolation = PoseInterpolator(
            InterpolationPolicy.LINEAR,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            clock_ns=clock,
        )
        renderer = Renderer.__new__(Renderer)
        renderer._commands = queue.Queue()
        renderer._sessions = {
            "session-1": SessionRuntime(
                binding=SessionBinding("session-1", "epoch-1"),
                pose=pose,
                interpolation=interpolation,
            )
        }
        renderer._scenes = Mock()
        renderer._cameras = Mock()
        renderer._cameras.render_viewports.return_value = ()
        renderer._layers = Mock()

        renderer.tick()
        clock.now = 50_000_000
        renderer.tick()
        clock.now = 75_000_000
        renderer.tick()

        scene_frame = renderer._scenes.apply_pose.call_args.args[0]
        camera_frames = renderer._cameras.tick.call_args.args[0]
        self.assertIs(scene_frame, camera_frames["session-1"])
        self.assertEqual(scene_frame.simulation_timestamp_ns, 25_000_000)
        self.assertEqual(scene_frame.entities[0].position_enu_m[0], 5.0)

    def test_pose_source_status_exposes_bounded_interpolation_diagnostics(
        self,
    ) -> None:
        interpolation = PoseInterpolator(
            InterpolationPolicy.LINEAR,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            clock_ns=lambda: 0,
        )
        renderer = Renderer.__new__(Renderer)
        renderer._sessions = {
            "session-1": SessionRuntime(
                binding=SessionBinding("session-1", "epoch-1"),
                pose_binding=Mock(),
                interpolation=interpolation,
            )
        }

        result = renderer._execute(
            Mock(
                operation="get_pose_source",
                session_id="session-1",
            )
        )

        self.assertEqual(result.status, 200)
        assert result.body is not None
        self.assertEqual(result.body["policy"], "linear")
        self.assertEqual(result.body["state"], "reset")
        self.assertEqual(result.body["discontinuityResetCount"], 0)
        self.assertNotIn("producer", json.dumps(result.body))
        self.assertNotIn("spiffe", json.dumps(result.body))

    def test_authorization_renewal_and_revocation_reset_interpolation(
        self,
    ) -> None:
        binding = SessionBinding("session-1", "epoch-1")
        scene_body = json.loads(
            (
                Path(__file__).resolve().parents[2]
                / "fixtures/anonymous-scene-body.json"
            ).read_text(encoding="utf-8")
        )
        scene_body["sessionId"] = binding.session_id
        scene_body["epochId"] = binding.epoch_id
        scene = SceneBinding.parse(
            {"body": scene_body, "digest": f"sha256:{'1' * 64}"}
        )
        source = PoseSourceBinding(
            session_id=binding.session_id,
            epoch_id=binding.epoch_id,
            frame_uri=scene.frame_uri,
            frame_digest=scene.frame_digest,
            entity_table_revision=1,
            entity_table_digest=f"sha256:{'2' * 64}",
            maximum_entities=20,
            maximum_message_bytes=4 * 1024 * 1024,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            producer_id="producer-1",
            producer_spiffe_id="spiffe://example.test/producer-1",
            authorization_revision=1,
            expires_at="2026-08-03T03:00:00Z",
            revoked=False,
        )
        interpolation = PoseInterpolator(
            scene.interpolation,
            source.maximum_cadence_hz,
            source.stale_after_ms,
            clock_ns=lambda: 0,
        )
        mirror = Mock()
        renderer = Renderer.__new__(Renderer)
        renderer._config = Mock(pose_directory=Path("/unused"))
        renderer._scenes = Mock()
        renderer._sessions = {
            binding.session_id: SessionRuntime(
                binding=binding,
                scene=scene,
                pose=mirror,
                pose_binding=source,
                interpolation=interpolation,
            )
        }
        renewed = replace(
            source,
            authorization_revision=2,
            expires_at="2026-08-03T04:00:00Z",
        )

        renderer._execute(
            Mock(
                operation="put_pose_source",
                session_id=binding.session_id,
                value=renewed,
            )
        )

        mirror.renew.assert_called_once_with(renewed)
        self.assertEqual(
            interpolation.diagnostics().last_reset_reason,
            InterpolationResetReason.AUTHORIZATION_REVISION_CHANGED,
        )
        revoked = replace(
            renewed,
            authorization_revision=3,
            revoked=True,
        )

        renderer._execute(
            Mock(
                operation="put_pose_source",
                session_id=binding.session_id,
                value=revoked,
            )
        )

        mirror.revoke.assert_called_once_with()
        renderer._scenes.mark_pose_stale.assert_called_once_with(
            binding.session_id
        )
        self.assertEqual(
            interpolation.diagnostics().last_reset_reason,
            InterpolationResetReason.REVOKED,
        )

    def test_changed_pose_binding_requires_a_new_authorization_revision(
        self,
    ) -> None:
        binding = SessionBinding.parse({"sessionId": "session-1", "epochId": "epoch-1"})
        scene_body = json.loads(
            (
                Path(__file__).resolve().parents[2]
                / "fixtures/anonymous-scene-body.json"
            ).read_text(encoding="utf-8")
        )
        scene_body["sessionId"] = binding.session_id
        scene_body["epochId"] = binding.epoch_id
        scene = SceneBinding.parse({"body": scene_body, "digest": f"sha256:{'1' * 64}"})
        pose = PoseSourceBinding(
            session_id=binding.session_id,
            epoch_id=binding.epoch_id,
            frame_uri=scene.frame_uri,
            frame_digest=scene.frame_digest,
            entity_table_revision=1,
            entity_table_digest=f"sha256:{'2' * 64}",
            maximum_entities=20,
            maximum_message_bytes=4 * 1024 * 1024,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            producer_id="producer-1",
            producer_spiffe_id="spiffe://example.test/producer-1",
            authorization_revision=4,
            expires_at="2026-08-03T03:00:00Z",
            revoked=False,
        )
        mirror = Mock()
        renderer = Renderer.__new__(Renderer)
        renderer._sessions = {
            binding.session_id: SessionRuntime(
                binding=binding,
                scene=scene,
                pose=mirror,
                pose_binding=pose,
            )
        }
        changed = replace(pose, expires_at="2026-08-03T04:00:00Z")

        with self.assertRaisesRegex(
            ContractError, "authorization revision is immutable"
        ):
            renderer._execute(
                Mock(
                    operation="put_pose_source",
                    session_id=binding.session_id,
                    value=changed,
                )
            )

        mirror.renew.assert_not_called()

    def test_private_bindings_are_exact_and_typed(self) -> None:
        session = SessionBinding.parse({"sessionId": "session-1", "epochId": "epoch-1"})
        self.assertEqual(session.session_id, "session-1")
        with self.assertRaises(ContractError):
            SessionBinding.parse(
                {
                    "sessionId": "session-1",
                    "epochId": "epoch-1",
                    "owner": "must-not-cross-runtime-boundary",
                }
            )

        camera = CameraBinding.parse(
            {
                "sessionId": "session-1",
                "cameraId": "camera-1",
                "revision": 1,
                "renderSlot": 2,
                "definition": {
                    "rig": {
                        "kind": "look_at",
                        "eyeM": {"x": 4.0, "y": -4.0, "z": 3.0},
                        "targetM": {"x": 0.0, "y": 0.0, "z": 0.0},
                    },
                    "widthPx": 1280,
                    "heightPx": 720,
                    "frameRateMillihertz": 20_000,
                    "verticalFovDegrees": 60.0,
                    "nearClipM": 0.1,
                    "farClipM": 10_000.0,
                    "streamPolicy": "on_demand",
                    "recordingPolicy": "disabled",
                },
            },
            4,
        )
        self.assertEqual(camera.render_slot, 2)

    def test_artifacts_are_content_addressed_and_self_contained(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = root / "artifacts"
            cache = root / "cache"
            (artifacts / "sha256").mkdir(parents=True)
            payload = b'#usda 1.0\n\ndef Xform "Root" {}\n'
            digest = hashlib.sha256(payload).hexdigest()
            path = artifacts / "sha256" / f"{digest}.usd"
            path.write_bytes(payload)
            resolved = ArtifactStore(artifacts, cache).resolve(
                {
                    "artifactUri": "artifact://fixture/world",
                    "digest": f"sha256:{digest}",
                    "format": "usd",
                    "byteLength": len(payload),
                }
            )
            self.assertEqual(resolved.path, path)
            path.write_bytes(payload + b" ")
            with self.assertRaises(ContractError):
                ArtifactStore(artifacts, cache).resolve(
                    {
                        "artifactUri": "artifact://fixture/world",
                        "digest": f"sha256:{digest}",
                        "format": "usd",
                        "byteLength": len(payload),
                    }
                )

    def test_artifact_ingest_hashes_and_materializes_atomically(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory) / "artifacts"
            (artifacts / "sha256").mkdir(parents=True)
            materializer = ArtifactMaterializer(artifacts, 1024)
            payload = b'#usda 1.0\n\ndef Xform "Root" {}\n'
            digest = hashlib.sha256(payload).hexdigest()
            path = materializer.materialize(
                digest, "usd", len(payload), BytesIO(payload)
            )
            self.assertEqual(path.read_bytes(), payload)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

            repeated = materializer.materialize(
                digest, "usd", len(payload), BytesIO(payload)
            )
            self.assertEqual(repeated, path)
            self.assertFalse(
                any(
                    candidate.name.endswith(".next")
                    for candidate in (artifacts / "sha256").iterdir()
                )
            )

            wrong_digest = "1" * 64
            with self.assertRaises(ContractError):
                materializer.materialize(
                    wrong_digest, "usd", len(payload), BytesIO(payload)
                )
            self.assertFalse((artifacts / "sha256" / f"{wrong_digest}.usd").exists())

    def test_pose_decoder_rejects_binding_mismatch(self) -> None:
        entity_id = b"entity-1"
        table_hasher = hashlib.sha256()
        table_hasher.update(struct.pack(">QH", 1, len(entity_id)))
        table_hasher.update(entity_id)
        table_digest = table_hasher.digest()
        frame_digest = bytes.fromhex("11" * 32)
        session = b"session-1"
        epoch = b"epoch-1"
        frame = b"frames://world/synthetic/revision/r1"
        entity = (
            struct.pack(">HBB", len(entity_id), 0x03, 0)
            + struct.pack(">7d", 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0)
            + entity_id
        )
        header = (
            b"VVPOSE01"
            + struct.pack(">HHI", 1, 0, 0)
            + struct.pack(
                ">QqQIHHHH",
                1,
                10_000_000,
                1,
                1,
                len(session),
                len(epoch),
                len(frame),
                1,
            )
            + frame_digest
            + table_digest
            + session
            + epoch
            + frame
        )
        encoded = bytearray(header + entity)
        encoded[12:16] = struct.pack(">I", len(encoded))
        binding = PoseSourceBinding.parse(
            {
                "schemaVersion": ("veoveo.io/simulation-view-pose-ingress-control/v2"),
                "sessionId": "session-1",
                "epochId": "epoch-1",
                "frameRevision": {
                    "uri": frame.decode(),
                    "digest": f"sha256:{frame_digest.hex()}",
                },
                "entityTableRevision": 1,
                "entityTableDigest": f"sha256:{table_digest.hex()}",
                "limits": {
                    "maximumEntities": 8,
                    "maximumMessageBytes": 65536,
                    "maximumCadenceHz": 120,
                    "staleAfterMs": 500,
                },
                "producer": {
                    "producerId": "fixture",
                    "spiffeId": "spiffe://example.test/fixture",
                    "authorizationRevision": 1,
                    "expiresAt": "2026-07-26T12:00:00Z",
                    "revoked": False,
                },
            }
        )
        snapshot = decode_snapshot(bytes(encoded), binding)
        self.assertEqual(snapshot.entities[0].position_enu_m, (1.0, 2.0, 3.0))
        wrong = PoseSourceBinding(
            session_id="session-2",
            epoch_id=binding.epoch_id,
            frame_uri=binding.frame_uri,
            frame_digest=binding.frame_digest,
            entity_table_revision=binding.entity_table_revision,
            entity_table_digest=binding.entity_table_digest,
            maximum_entities=binding.maximum_entities,
            maximum_message_bytes=binding.maximum_message_bytes,
            maximum_cadence_hz=binding.maximum_cadence_hz,
            stale_after_ms=binding.stale_after_ms,
            producer_id=binding.producer_id,
            producer_spiffe_id=binding.producer_spiffe_id,
            authorization_revision=binding.authorization_revision,
            expires_at=binding.expires_at,
            revoked=binding.revoked,
        )
        with self.assertRaises(ContractError):
            decode_snapshot(bytes(encoded), wrong)

    def test_pose_authorization_renewal_preserves_reader_and_latest_state(
        self,
    ) -> None:
        binding = PoseSourceBinding(
            session_id="session-1",
            epoch_id="epoch-1",
            frame_uri="frames://world/synthetic/revision/r1",
            frame_digest=f"sha256:{'1' * 64}",
            entity_table_revision=1,
            entity_table_digest=f"sha256:{'2' * 64}",
            maximum_entities=8,
            maximum_message_bytes=65536,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            producer_id="fixture",
            producer_spiffe_id="spiffe://example.test/fixture",
            authorization_revision=1,
            expires_at="2026-08-02T12:00:00Z",
            revoked=False,
        )
        reader = object()
        latest = object()
        mirror = PoseMirror.__new__(PoseMirror)
        mirror._directory = Path("/unused")
        mirror._binding = binding
        mirror._reader = reader
        mirror._generation = 7
        mirror._latest = latest
        mirror._accepted_at = 4.0

        mirror.renew(
            replace(
                binding,
                authorization_revision=2,
                expires_at="2026-08-02T12:05:00Z",
            )
        )

        self.assertIs(mirror._reader, reader)
        self.assertIs(mirror._latest, latest)
        self.assertEqual(mirror._generation, 7)
        with self.assertRaisesRegex(ContractError, "authorization revision is stale"):
            mirror.renew(binding)


if __name__ == "__main__":
    unittest.main()
