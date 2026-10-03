from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

from pymavlink import mavutil

from veoveo_uav_sim.contracts import parse_command
from veoveo_uav_sim.px4 import CommandDeadline, Px4Commander
from veoveo_uav_sim.server import AdapterApplication


class Px4CommandDeadlineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.now = 100.0
        self.clock = patch("veoveo_uav_sim.px4.time.monotonic", lambda: self.now)
        self.clock.start()
        self.addCleanup(self.clock.stop)
        self.connection = Mock()
        self.commander = Px4Commander(instance=0, origin_height_m=-17.0)
        self.commander._connection = self.connection
        self.commander._connected = True

    def test_expired_command_never_sends(self) -> None:
        with self.assertRaisesRegex(TimeoutError, "inspect vehicle state"):
            self.commander.takeoff(197.0, deadline=CommandDeadline(self.now))
        self.connection.mav.command_long_send.assert_not_called()

    def test_contended_lock_cannot_outlive_command(self) -> None:
        self.commander._lock.acquire()
        try:
            with self.assertRaisesRegex(TimeoutError, "lock deadline"):
                self.commander.arm(deadline=CommandDeadline.after(0.001))
        finally:
            self.commander._lock.release()
        self.connection.mav.command_long_send.assert_not_called()

    def test_missing_ack_is_not_retried_or_followed_by_takeoff(self) -> None:
        def receive(*, blocking: bool, timeout: float):
            self.assertTrue(blocking)
            self.now += timeout
            return None

        self.connection.recv_match.side_effect = receive
        with self.assertRaisesRegex(TimeoutError, "acknowledge"):
            self.commander.takeoff(197.0, deadline=CommandDeadline.after(2.5))
        self.assertEqual(self.now, 102.5)
        self.assertEqual(self.connection.mav.command_long_send.call_count, 1)
        self.assertEqual(
            self.connection.mav.command_long_send.call_args.args[2],
            mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
        )

    def test_arming_retries_only_definitive_temporary_rejections(self) -> None:
        def receive(*, blocking: bool, timeout: float):
            self.now += timeout
            return SimpleNamespace(
                get_type=lambda: "COMMAND_ACK",
                command=mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
                result=mavutil.mavlink.MAV_RESULT_TEMPORARILY_REJECTED,
            )

        self.connection.recv_match.side_effect = receive
        with self.assertRaisesRegex(TimeoutError, "before the command deadline"):
            self.commander.takeoff(197.0, deadline=CommandDeadline.after(2.5))
        self.assertEqual(self.now, 102.5)
        self.assertEqual(self.connection.mav.command_long_send.call_count, 2)
        self.assertTrue(all(
            call.args[2] == mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM
            for call in self.connection.mav.command_long_send.call_args_list
        ))

    def test_mode_transition_consumes_the_same_arming_budget(self) -> None:
        self.commander._landed_state = mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND
        _, self.commander._px4_main_mode, self.commander._px4_sub_mode = mavutil.px4_map["LAND"]

        def receive(*, blocking: bool, timeout: float):
            self.now += timeout
            return SimpleNamespace(
                get_type=lambda: "COMMAND_ACK",
                command=mavutil.mavlink.MAV_CMD_DO_SET_MODE,
                result=mavutil.mavlink.MAV_RESULT_ACCEPTED,
            )

        self.connection.recv_match.side_effect = receive
        with self.assertRaisesRegex(TimeoutError, "enter main mode"):
            self.commander.takeoff(197.0, deadline=CommandDeadline.after(2.5))
        self.assertEqual(self.now, 102.5)
        self.assertEqual(self.connection.mav.command_long_send.call_count, 1)

    def test_interrupted_takeoff_rearms_from_reported_land_mode(self) -> None:
        def heartbeat(mode: str, *, armed: bool = False):
            _, main, sub = mavutil.px4_map[mode]
            return SimpleNamespace(
                get_type=lambda: "HEARTBEAT", get_srcSystem=lambda: 1,
                base_mode=(mavutil.mavlink.MAV_MODE_FLAG_CUSTOM_MODE_ENABLED
                           | (mavutil.mavlink.MAV_MODE_FLAG_SAFETY_ARMED if armed else 0)),
                custom_mode=(main << 16) | (sub << 24),
            )

        def accepted(command: int):
            return SimpleNamespace(
                get_type=lambda: "COMMAND_ACK", command=command,
                result=mavutil.mavlink.MAV_RESULT_ACCEPTED,
            )

        # The installed failure landed during takeoff, before IN_AIR was seen.
        self.commander._consume(heartbeat("LAND"))
        for state in (mavutil.mavlink.MAV_LANDED_STATE_TAKEOFF,
                      mavutil.mavlink.MAV_LANDED_STATE_LANDING,
                      mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND):
            self.commander._consume(SimpleNamespace(
                get_type=lambda: "EXTENDED_SYS_STATE", landed_state=state,
            ))
        self.assertFalse(self.commander._has_flown)
        self.connection.recv_match.side_effect = [
            accepted(mavutil.mavlink.MAV_CMD_DO_SET_MODE), heartbeat("LOITER"),
            accepted(mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM), heartbeat("LOITER", armed=True),
            accepted(mavutil.mavlink.MAV_CMD_NAV_TAKEOFF),
        ]
        self.commander.takeoff(197.0, deadline=CommandDeadline.after(10.0))
        calls = self.connection.mav.command_long_send.call_args_list
        self.assertEqual([call.args[2] for call in calls], [
            mavutil.mavlink.MAV_CMD_DO_SET_MODE,
            mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
            mavutil.mavlink.MAV_CMD_NAV_TAKEOFF,
        ])
        self.assertEqual(calls[0].args[4:7], mavutil.px4_map["LOITER"])

    def test_rearming_preserves_airborne_and_other_selected_modes(self) -> None:
        for state, mode in (
            (mavutil.mavlink.MAV_LANDED_STATE_IN_AIR, "LAND"),
            (mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND, "LOITER"),
        ):
            with self.subTest(state=state, mode=mode):
                self.commander._has_flown = True
                self.commander._landed_state = state
                _, self.commander._px4_main_mode, self.commander._px4_sub_mode = mavutil.px4_map[mode]
                with patch.object(self.commander, "_arm_when_ready_locked") as arm:
                    self.commander.arm(deadline=CommandDeadline.after(10.0))
                    arm.assert_called_once()
                self.connection.mav.command_long_send.assert_not_called()

    def test_no_takeoff_after_arming_consumes_deadline(self) -> None:
        def finish_arming(deadline: CommandDeadline) -> None:
            self.commander._armed = True
            self.now = deadline.expires_at

        with patch.object(self.commander, "_arm_locked", finish_arming):
            with self.assertRaisesRegex(TimeoutError, "inspect vehicle state"):
                self.commander.takeoff(197.0, deadline=CommandDeadline.after(2.5))
        self.connection.mav.command_long_send.assert_not_called()

    def test_adapter_takeover_consumes_vehicle_command_budget(self) -> None:
        application = AdapterApplication.__new__(AdapterApplication)
        application._state = Mock()
        application._fleet_loop = Mock()
        application._commanders = {"uav-1": self.commander}

        def take_control(_vehicle_ids, *, timeout_seconds: float) -> None:
            self.assertEqual(timeout_seconds, 30.0)
            self.now += 25.0

        application._fleet_loop.take_control.side_effect = take_control
        with patch.object(self.commander, "takeoff") as takeoff:
            application._execute_command(parse_command({
                "session_id": "uav-showcase", "vehicle_id": "uav-1",
                "command": "takeoff", "relative_altitude_m": 197.0,
            }))
        self.assertEqual(takeoff.call_args.kwargs["deadline"].remaining(), 50.0)

    def test_mission_admission_has_a_bounded_arming_deadline(self) -> None:
        def receive(*, blocking: bool, timeout: float):
            self.now += timeout
            return None

        self.connection.recv_match.side_effect = receive
        with self.assertRaisesRegex(TimeoutError, "acknowledge"):
            self.commander.execute_mission(())
        self.assertEqual(self.now, 115.0)
        self.assertEqual(self.connection.mav.command_long_send.call_count, 1)


if __name__ == "__main__":
    unittest.main()
