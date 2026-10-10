from __future__ import annotations

from collections import deque
from contextlib import ExitStack
import sys
import threading
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from veoveo_uav_sim.hydra_camera import (
    NativeH264CameraSensor, RetainedProductAcquisitionError, RtxHydraRenderProduct,
    RtxTiledHydraRenderProduct,
)
from veoveo_uav_sim.operator_products import OperatorCameraProduct
from test_operator_products import _camera, _config


class PhysicalProductLifecycleTests(unittest.TestCase):
    def _modules(self, stack, dispatcher):
        omni = ModuleType("omni")
        omni.hydratexture = SimpleNamespace(GLOBAL_EVENT_DRAWABLE_CHANGED="drawable")
        carb = ModuleType("carb")
        events = ModuleType("carb.eventdispatcher")
        events.get_eventdispatcher = lambda: dispatcher
        stack.enter_context(patch.dict(sys.modules, {"omni": omni, "omni.hydratexture": omni.hydratexture, "carb": carb, "carb.eventdispatcher": events}))

    def test_full_post_attach_acquisition_retires_both_sensor_and_atlas_on_failures(self):
        for owner in ("sensor", "atlas"):
            for stage in ("subscription", "updates"):
                for failed_cleanup in (False, True):
                    with self.subTest(owner=owner, stage=stage, failed_cleanup=failed_cleanup), ExitStack() as stack:
                        dispatcher = Mock()
                        self._modules(stack, dispatcher)
                        render = Mock(path="/Render/test", width=1280, height=720)
                        writer = Mock()
                        if stage == "subscription": dispatcher.observe_event.side_effect = RuntimeError("subscription failed")
                        else: render.set_updates_enabled.side_effect = RuntimeError("updates failed")
                        if failed_cleanup: writer.detach.side_effect = [RuntimeError("detach failed"), None]
                        module = "hydra_camera" if owner == "sensor" else "operator_products"
                        constructor = "RtxHydraRenderProduct" if owner == "sensor" else "RtxTiledHydraRenderProduct"
                        stack.enter_context(patch(f"veoveo_uav_sim.{module}.{constructor}", return_value=render))
                        stack.enter_context(patch(f"veoveo_uav_sim.{module}.attach_native_rtsp_writer", return_value=writer))
                        def acquire():
                            if owner == "sensor":
                                return NativeH264CameraSensor(name="test", camera_path="/Camera/test", width=1280, height=720, render_fps=16, rtsp_port=8560)
                            config = _config([_camera("follow", 0, {"kind":"fixed", "pose":{"positionM":{"x":0,"y":0,"z":0}, "orientationXyzw":{"x":0,"y":0,"z":0,"w":1}}})])
                            cameras = SimpleNamespace(cameras=[SimpleNamespace(definition=SimpleNamespace(camera_id="follow"), camera_path="/Camera/test")])
                            return OperatorCameraProduct(config, cameras)
                        if failed_cleanup:
                            with self.assertRaises(RetainedProductAcquisitionError) as caught: acquire()
                            retained = caught.exception.owner
                            self.assertFalse(retained.cleanup_complete)
                            self.assertIs(retained._writer, writer)
                            with self.assertRaisesRegex(RuntimeError,"detach failed"): retained.close()
                            self.assertTrue(retained.cleanup_complete)
                            self.assertIsNone(retained._writer)
                            self.assertIsNone(retained._render_product)
                            with self.assertRaisesRegex(RuntimeError,"detach failed"): retained.close()
                            self.assertEqual(writer.detach.call_count,2)
                        else:
                            with self.assertRaisesRegex(RuntimeError, f"{stage} failed"): acquire()
                            writer.detach.assert_called_once()
                        render.close.assert_called_once()

    def test_sensor_receiver_join_failure_retains_handle_and_first_failure_after_retry(self):
        sensor = NativeH264CameraSensor.__new__(NativeH264CameraSensor)
        sensor._lock = threading.Lock(); sensor._cleanup_lock = threading.Lock()
        sensor._cleanup_failure = None; sensor._closed = False
        sensor._latest = object(); sensor._rendered_samples = deque([object()]); sensor._access_units = deque([object()])
        receiver = sensor._receiver = Mock(); receiver.close.side_effect = [RuntimeError("join failed"),None]
        writer = sensor._writer = Mock(); render = sensor._render_product = Mock(); sensor._subscription = object()
        with self.assertRaisesRegex(RuntimeError,"join failed"): sensor.close()
        self.assertIs(sensor._receiver, receiver); self.assertFalse(sensor.cleanup_complete)
        self.assertIsNone(sensor._latest); self.assertFalse(sensor._rendered_samples); self.assertFalse(sensor._access_units)
        with self.assertRaisesRegex(RuntimeError,"join failed"): sensor.close()
        self.assertTrue(sensor.cleanup_complete)
        writer.detach.assert_called_once(); render.close.assert_called_once()
        self.assertEqual(receiver.close.call_count,2)

    def test_texture_retirement_failure_keeps_handle_and_sticky_failure_after_terminal_retry(self):
        for cls in (RtxHydraRenderProduct, RtxTiledHydraRenderProduct):
            product = cls.__new__(cls); product._hydra_texture = object(); product._path = "/Render/test"; product._cleanup_failure = None
            original = product._hydra_texture
            with patch("veoveo_uav_sim.hydra_camera._retire_owned_hydra_texture", side_effect=[RuntimeError("texture busy"),None]) as retire:
                with self.assertRaisesRegex(RuntimeError,"texture busy"): product.close()
                self.assertIs(product._hydra_texture,original); self.assertFalse(product.cleanup_complete)
                with self.assertRaisesRegex(RuntimeError,"texture busy"): product.close()
                self.assertTrue(product.cleanup_complete)
                with self.assertRaisesRegex(RuntimeError,"texture busy"): product.close()
                self.assertEqual(retire.call_count,2)

    def test_constructor_post_texture_failure_retains_exact_texture_for_final_retry(self):
        for cls in (RtxHydraRenderProduct,RtxTiledHydraRenderProduct):
            for stage in ("path", "relationship"):
                if cls is RtxHydraRenderProduct and stage == "relationship": continue
                with self.subTest(product=cls.__name__,stage=stage), ExitStack() as stack:
                    texture = Mock()
                    texture.get_render_product_path.return_value = "/Render/OmniverseKit/HydraTextures/test"
                    if stage == "path": texture.get_render_product_path.side_effect = RuntimeError("texture path failed")
                    stage_object = Mock()
                    stage_object.GetPrimAtPath.return_value.GetRelationship.return_value.SetTargets.side_effect = RuntimeError("relationship failed")
                    from contextlib import nullcontext
                    modules={
                        "omni": ModuleType("omni"),
                        "omni.usd": SimpleNamespace(get_context=lambda:SimpleNamespace(get_stage=lambda:stage_object)),
                        "omni.kit.hydra_texture": SimpleNamespace(create_hydra_texture=lambda *args,**kwargs:texture),
                        "isaacsim.core.experimental.objects": SimpleNamespace(Camera=Mock()),
                        "carb": SimpleNamespace(settings=SimpleNamespace(get_settings=Mock())),
                        "pxr": SimpleNamespace(Usd=SimpleNamespace(EditContext=lambda *args:nullcontext())),
                    }
                    modules["omni"].usd=modules["omni.usd"]
                    stack.enter_context(patch.dict(sys.modules,modules))
                    retire=stack.enter_context(patch("veoveo_uav_sim.hydra_camera._retire_owned_hydra_texture",side_effect=[RuntimeError("retirement failed"),None]))
                    args={"name":"test","render_fps":16}
                    if cls is RtxHydraRenderProduct:args.update(camera_path="/Camera/test",width=1280,height=720)
                    else:args.update(camera_paths=("/Camera/test",),tile_width=1280,tile_height=720)
                    with self.assertRaises(RetainedProductAcquisitionError) as caught:cls(**args)
                    self.assertRegex(str(caught.exception.__cause__),f"{stage} failed")
                    retained=caught.exception.owner
                    self.assertIs(retained._hydra_texture,texture)
                    with self.assertRaisesRegex(RuntimeError,"retirement failed"):retained.close()
                    self.assertTrue(retained.cleanup_complete)
                    self.assertEqual(retire.call_count,2)

    def test_terminal_child_sticky_failure_does_not_leave_false_live_parent_handle(self):
        sensor = NativeH264CameraSensor.__new__(NativeH264CameraSensor)
        sensor._lock=threading.Lock();sensor._cleanup_lock=threading.Lock();sensor._cleanup_failure=None
        sensor._closed=False;sensor._latest=None;sensor._rendered_samples=deque();sensor._access_units=deque()
        sensor._receiver=None;sensor._writer=None;sensor._subscription=None
        child=RtxHydraRenderProduct.__new__(RtxHydraRenderProduct)
        child._hydra_texture=object();child._path="/Render/test";child._cleanup_failure=None
        sensor._render_product=child
        with patch("veoveo_uav_sim.hydra_camera._retire_owned_hydra_texture",side_effect=[RuntimeError("retirement failed"),None]):
            with self.assertRaisesRegex(RuntimeError,"retirement failed"):sensor.close()
            self.assertFalse(sensor.cleanup_complete)
            with self.assertRaisesRegex(RuntimeError,"retirement failed"):sensor.close()
            self.assertTrue(child.cleanup_complete);self.assertTrue(sensor.cleanup_complete)

    def test_nested_writer_and_outer_render_failures_remain_owned_through_retry(self):
        from veoveo_uav_sim.hydra_camera import _failed_acquisition

        class Owner:
            def __init__(self, name):
                self.name = name
                self.calls = 0
                self.cleanup_complete = False

            def close(self):
                self.calls += 1
                if self.calls == 1:
                    raise RuntimeError(self.name)
                self.cleanup_complete = True

        writer = Owner("writer detach failed")
        render = Owner("render retirement failed")
        original = RetainedProductAcquisitionError(writer)
        with self.assertRaises(RetainedProductAcquisitionError) as caught:
            _failed_acquisition(render, original)
        retained = caught.exception.owner
        self.assertIs(caught.exception.__cause__, original)
        self.assertFalse(retained.cleanup_complete)
        self.assertEqual((writer.calls, render.calls), (1, 1))
        with self.assertRaisesRegex(RuntimeError, "writer detach failed"):
            retained.close()
        self.assertTrue(retained.cleanup_complete)
        self.assertEqual((writer.calls, render.calls), (2, 2))
        with self.assertRaisesRegex(RuntimeError, "writer detach failed"):
            retained.close()
        self.assertEqual((writer.calls, render.calls), (2, 2))
