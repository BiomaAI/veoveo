from __future__ import annotations

from contextlib import nullcontext
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from veoveo_uav_sim.hydra_camera import RtxHydraRenderProduct, RtxTiledHydraRenderProduct


class HydraRetirementTests(unittest.TestCase):
    def test_repeated_generations_release_texture_product_and_settings(self) -> None:
        stage = Mock()
        settings = Mock()
        modules = {
            "carb": SimpleNamespace(settings=SimpleNamespace(get_settings=lambda: settings)),
            "omni": SimpleNamespace(usd=SimpleNamespace(get_context=lambda: SimpleNamespace(get_stage=lambda: stage))),
            "omni.usd": SimpleNamespace(get_context=lambda: SimpleNamespace(get_stage=lambda: stage)),
            "pxr": SimpleNamespace(Usd=SimpleNamespace(EditContext=lambda *args: nullcontext())),
        }
        with patch.dict("sys.modules", modules):
            for product_type in (RtxHydraRenderProduct, RtxTiledHydraRenderProduct):
                for generation in range(1, 4):
                    product = product_type.__new__(product_type)
                    product._path = f"/Render/owned_g{generation}"
                    texture = Mock()
                    texture.get_settings_path.return_value = f"/hydra/owned_g{generation}"
                    product._hydra_texture = texture
                    product._cleanup_failure = None
                    product.close()
                    product.close()
                    self.assertFalse(texture.updates_enabled)
                    self.assertIsNone(product._hydra_texture)
                    stage.RemovePrim.assert_called_with(product._path)
                    settings.destroy_item.assert_called_with(f"/hydra/owned_g{generation}")
        self.assertEqual(stage.RemovePrim.call_count, 6)
        self.assertEqual(settings.destroy_item.call_count, 6)
