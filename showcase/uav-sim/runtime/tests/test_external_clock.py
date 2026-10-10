from __future__ import annotations

import unittest

from veoveo_uav_sim.external_clock import ExternalSimulationClock


class Attribute:
    def __init__(self) -> None:
        self.value = 0.0

    def Set(self, value: float) -> None:
        self.value = value

    def Get(self) -> float:
        return self.value


class Settings:
    def __init__(self) -> None:
        self.values = {"/app/player/playSimulations": True}

    def set_bool(self, path: str, value: bool) -> None:
        self.values[path] = value

    def get_as_bool(self, path: str) -> bool:
        return self.values.get(path, False)


class Manager:
    def __init__(self) -> None:
        self.samples = 1
        self.steps = 0

    def get_sample_count(self) -> int:
        return self.samples

    def get_num_physics_steps(self) -> int:
        return self.steps

    def get_simulation_time_at_time(self, time: tuple[int, int]) -> float:
        return time[0] / time[1]


class ExternalClockTests(unittest.TestCase):
    def setUp(self) -> None:
        self.attribute = Attribute()
        self.settings = Settings()
        self.manager = Manager()
        self.clock = ExternalSimulationClock(
            self.attribute, self.settings, self.manager, 240
        )
        self.clock.admit_initialized()

    def test_completed_time_precedes_render_pause_and_coalesced_steps(self) -> None:
        seen = []
        def render() -> None:
            seen.append((self.attribute.value,
                         self.settings.get_as_bool("/app/player/playSimulations")))
        for step in (1, 10, 10, 240):
            observation = self.clock.render(step, render)
            self.assertEqual(observation.physics_step, step)
            self.assertEqual(observation.native_physics_steps, 0)
        self.assertEqual(seen, [(1/240, False), (10/240, False),
                                (10/240, False), (1.0, False)])
        self.assertTrue(self.settings.get_as_bool("/app/player/playSimulations"))
        with self.assertRaisesRegex(RuntimeError, "generation reset"):
            self.clock.render(0, render)

    def test_native_step_or_missing_samples_refuses_clock_claim(self) -> None:
        def accidental_step() -> None:
            self.manager.steps += 1
        with self.assertRaisesRegex(RuntimeError, "native physics"):
            self.clock.render(1, accidental_step)
        self.assertTrue(self.settings.get_as_bool("/app/player/playSimulations"))
        with self.assertRaisesRegex(RuntimeError, "not initialized"):
            self.clock.render(2, lambda: self.fail("publication after failed clock"))
        self.setUp()
        self.manager.samples = 0
        with self.assertRaisesRegex(RuntimeError, "samples"):
            self.clock.render(1, lambda: self.fail("uninitialized render"))

    def test_supported_lifecycle_reseeds_samples_without_tensor_rebind(self) -> None:
        identities = (object(), object(), object(), object())
        events = []
        self.clock.render(240, lambda: None)
        def stop() -> None:
            events.append("stop")
            self.manager.samples = 0
        def play() -> None:
            events.append("play")
            self.manager.samples = 1
        def update() -> None:
            self.assertFalse(self.settings.get_as_bool("/app/player/playSimulations"))
            events.append("update")
        self.clock.reset_generation(stop, play, update,
                                    lambda: events.append("warp-reset"),
                                    lambda: identities)
        self.assertEqual(events, ["stop", "update", "warp-reset", "play", "update"])
        self.assertEqual(self.clock.render(0, lambda: None).generation, 2)
        self.assertTrue(self.settings.get_as_bool("/app/player/playSimulations"))

    def test_changed_tensor_identity_stops_before_reset_or_publication(self) -> None:
        identity = [object()]
        def stop() -> None:
            identity[0] = object()
        with self.assertRaisesRegex(RuntimeError, "replaced external"):
            self.clock.reset_generation(stop, lambda: None, lambda: None,
                                        lambda: self.fail("unsafe reset"),
                                        lambda: tuple(identity))
        with self.assertRaisesRegex(RuntimeError, "not initialized"):
            self.clock.render(0, lambda: self.fail("unsafe publication"))

    def test_failed_fabric_write_is_not_validated_by_reference_echo(self) -> None:
        self.attribute.Set = lambda value: None
        with self.assertRaisesRegex(RuntimeError, "did not retain"):
            self.clock.render(240, lambda: self.fail("unbound render"))

    def test_retired_generation_cannot_render_after_partial_stream_teardown(self) -> None:
        self.clock.render(1, lambda: None)
        self.clock.retire_generation()
        with self.assertRaisesRegex(RuntimeError, "not initialized"):
            self.clock.render(2, lambda: self.fail("partial reset publication"))
