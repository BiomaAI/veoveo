"""CPU controls for observational MAVLink feedback snapshots; no PX4 process."""
from __future__ import annotations

import tempfile
import threading
import unittest
from collections import deque

from pymavlink import mavutil

from veoveo_uav_sim.px4_hil import Px4HilBridge


class Px4HilObservationTests(unittest.TestCase):
    def test_atomic_feedback_snapshot_keeps_wire_time_flags_and_decoded_units(self) -> None:
        armed = mavutil.mavlink.MAV_MODE_FLAG_SAFETY_ARMED
        messages = deque([
            mavutil.mavlink.MAVLink_hil_actuator_controls_message(
                123456, [0.0, 0.25, 1.0, 2.0] + [0.0] * 12, armed, 1,
            ),
            mavutil.mavlink.MAVLink_hil_actuator_controls_message(
                123400, [1.0] * 16, 0, 0,
            ),
        ])

        class Connection:
            def close(self):
                pass

            def recv_match(self, *, blocking: bool):
                # Exactly one frame per drain call; the observation does not
                # infer timestamp ordering or an echo of the latest sensor.
                return messages.popleft() if messages else None

        with tempfile.TemporaryDirectory() as directory:
            bridge = Px4HilBridge(directory, instance=42)
            self.addCleanup(bridge.close)
            self.assertIsNone(bridge.actuator_observation())
            connection = Connection()
            bridge._connection = connection
            original = connection.recv_match
            count = 0

            def one_message(*, blocking: bool):
                nonlocal count
                count += 1
                return original(blocking=blocking) if count == 1 else None

            connection.recv_match = one_message
            bridge._drain_messages()
            first = bridge.actuator_observation()
            self.assertEqual(first.controls_rad_s, (100.0, 350.0, 1100.0, 1100.0))
            self.assertEqual(bridge.controls(), first.controls_rad_s)
            self.assertEqual((first.time_usec, first.flags, first.receive_sequence), (123456, 1, 1))
            self.assertGreater(first.received_monotonic_ns, 0)
            count = 0
            bridge._drain_messages()
            second = bridge.actuator_observation()
            self.assertEqual(second.controls_rad_s, (0.0, 0.0, 0.0, 0.0))
            self.assertEqual((second.time_usec, second.flags, second.receive_sequence), (123400, 0, 2))
            self.assertGreaterEqual(second.received_monotonic_ns, first.received_monotonic_ns)
            self.assertEqual(first.receive_sequence, 1)  # retained snapshot is immutable
            bridge.close()

    def test_readers_never_observe_metadata_from_another_control_message(self) -> None:
        armed = mavutil.mavlink.MAV_MODE_FLAG_SAFETY_ARMED
        with tempfile.TemporaryDirectory() as directory:
            bridge = Px4HilBridge(directory, instance=42)
            self.addCleanup(bridge.close)

            class Connection:
                def __init__(self):
                    self.sequence = 0

                def close(self):
                    pass

                def recv_match(self, *, blocking: bool):
                    self.sequence += 1
                    value = self.sequence
                    if value > 4096:
                        return None
                    return mavutil.mavlink.MAVLink_hil_actuator_controls_message(
                        value, [0.25 if value % 2 else 0.5] * 16, armed, value % 2,
                    )

            bridge._connection = Connection()
            ready = threading.Event()
            resume = threading.Event()

            def write():
                bridge._drain_messages()
                ready.set()
                if resume.wait(timeout=2):
                    for _ in range(16):
                        bridge._drain_messages()

            writer = threading.Thread(target=write)
            writer.start()
            try:
                self.assertTrue(ready.wait(timeout=2))
                self.assertTrue(writer.is_alive())
                self.assertEqual(bridge.actuator_observation().receive_sequence, 256)
            finally:
                resume.set()
            while writer.is_alive():
                observation = bridge.actuator_observation()
                if observation is not None:
                    self.assertEqual(observation.time_usec, observation.receive_sequence)
                    self.assertEqual(observation.flags, observation.receive_sequence % 2)
                    expected = 350.0 if observation.flags else 600.0
                    self.assertEqual(observation.controls_rad_s, (expected,) * 4)
            writer.join(timeout=2)
            final = bridge.actuator_observation()
            self.assertEqual(final.receive_sequence, 4096)
            # Always check the terminal snapshot even if the writer outruns the reader.
            self.assertEqual(final.controls_rad_s, (600.0,) * 4)
            bridge.close()
