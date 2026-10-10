"""CUDA plant and native PX4 takeoff, movement, landing and re-arm qualification.

Requires the repository-qualified PX4 tree in UAV_SIM_PX4_DIRECTORY, hardware CUDA,
MAVLINK20=1, and free ports for instance 42. Run with a 600-second outer timeout.
The fixture owns its process, sockets, thread and temporary writable root.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import shutil
import tempfile
import threading
import time
import unittest

import numpy as np
from pymavlink import mavutil

from test_plant import CudaPlant, NativeSensorProfile
from veoveo_uav_sim.contracts import Waypoint
from veoveo_uav_sim.px4 import CommandDeadline, Px4Commander
from veoveo_uav_sim.px4_hil import Px4HilBridge
from veoveo_uav_sim.realtime import MonotonicPhysicsClock
from veoveo_uav_sim.vehicle_spec import (
    PX4_IRIS_IMU_NOISE_REFERENCE_HZ, decode_hil_packet,
)


class Px4FlightTests(unittest.TestCase):
    def test_cuda_flight_can_land_and_rearm_with_healthy_compass(self) -> None:
        self.assertEqual(os.environ.get("MAVLINK20"), "1", "MAVLink 2 is required")
        root = Path(os.environ["UAV_SIM_PX4_DIRECTORY"])
        instance = 42
        profile = NativeSensorProfile.selected()
        schedule = os.environ.get("UAV_SIM_PX4_SCHEDULE", "paced")
        if schedule not in {"paced", "grouped-catch-up"}:
            raise ValueError("UAV_SIM_PX4_SCHEDULE must be paced or grouped-catch-up")
        if schedule == "grouped-catch-up" and profile.name != "held-30-60":
            raise ValueError("grouped-catch-up diagnoses the production held-30-60 profile")
        plant = CudaPlant(fleet_size=1, physics_hz=profile.physics_hz)
        plant.sample(1)  # CUDA compilation must finish before the sensor deadline starts.
        bridge = Px4HilBridge(str(root), instance=instance)
        commander = Px4Commander(instance=instance, origin_height_m=-17.0)
        stop = threading.Event()
        failures: list[BaseException] = []
        observations: list[dict[str, object]] = []
        latest: list[np.ndarray] = []
        samples: list[np.ndarray] = []
        control_samples: list[np.ndarray] = []
        sample_timing: list[tuple[float, float]] = []
        feedback_timing: list[dict[str, object]] = []
        stall_observations: list[dict[str, object]] = []
        catch_up_passes: list[dict[str, object]] = []
        saturation_seconds = np.zeros(4)
        saturation_run = np.zeros(4)
        longest_saturation = np.zeros(4)
        phase = ["preflight"]
        maxima = {"tiltDegrees": 0.0, "bodyRateDegreesPerSecond": 0.0}
        capture = Path(os.environ.get("UAV_SIM_PX4_FLIGHT_LOG_DIRECTORY") or
                       tempfile.mkdtemp(prefix="veoveo-px4-flight-"))
        capture.mkdir(parents=True, exist_ok=True)
        print(json.dumps({"profile": profile.name, "schedule": schedule, "stalls": stall_observations,
                        "physicsHz": profile.physics_hz,
                          "hilHz": profile.hil_hz, "capture": str(capture)}), flush=True)

        def check_truth(packet: np.ndarray, controls: np.ndarray, step: int) -> None:
            context = f"profile={profile.name} step={step} phase={phase[0]}"
            self.assertTrue(np.isfinite(packet).all(), f"nonfinite plant ground truth {context}")
            q = packet[3:7]
            self.assertLessEqual(abs(float(np.linalg.norm(q)) - 1.0), 1e-3,
                                 f"plant orientation quaternion lost normalization {context}")
            # Ground contact only: on-ground reaction impulses are outside flight acceptance.
            if packet[2] <= 0.10:
                return
            tilt_deg = float(np.degrees(np.arccos(np.clip(1-2*(q[0]**2+q[1]**2), -1, 1))))
            rate_deg_s = float(np.degrees(np.linalg.norm(packet[10:13])))
            maxima["tiltDegrees"] = max(maxima["tiltDegrees"], tilt_deg)
            maxima["bodyRateDegreesPerSecond"] = max(maxima["bodyRateDegreesPerSecond"], rate_deg_s)
            # bridge.controls() supplies decoded rotor rad/s, not PX4 normalized commands.
            # The owning decoder maps normalized*1000+100 and disarmed output to zero.
            self.assertTrue(np.isfinite(controls).all() and np.all((controls >= 0) & (controls <= 1100)),
                            f"invalid rotor command rad/s {controls.tolist()} {context}")
            normalized = np.clip((controls - 100.0) / 1000.0, 0.0, 1.0)
            saturated = (normalized <= 0.01) | (normalized >= 0.99)
            saturation_seconds[:] += saturated / profile.physics_hz
            saturation_run[:] = np.where(saturated, saturation_run + 1/profile.physics_hz, 0)
            longest_saturation[:] = np.maximum(longest_saturation, saturation_run)
            self.assertLess(tilt_deg, 60.0, f"ground-truth tilt={tilt_deg:.3f}deg {context}")
            self.assertLess(rate_deg_s, 360.0, f"ground-truth rate={rate_deg_s:.3f}deg/s {context}")


        def publish() -> None:
            try:
                started = time.monotonic()
                last_published_sensor_usec: int | None = None

                def sample(plant_step: int, hil_step: int) -> np.ndarray:
                    observation = bridge.actuator_observation()
                    controls = np.asarray(
                        observation.controls_rad_s if observation is not None else bridge.controls(),
                        dtype=np.float32,
                    )
                    plant.controls.assign(controls[None, :])
                    packet = plant.sample(plant_step)[0]
                    latest[:] = [packet]
                    samples.append(np.concatenate(([hil_step / profile.hil_hz], packet)))
                    control_samples.append(np.concatenate(([hil_step / profile.hil_hz], controls)))
                    sample_timing.append((hil_step / profile.hil_hz, time.monotonic() - started))
                    feedback_timing.append({
                        "plantStep": plant_step, "hilStep": hil_step,
                        "wallSeconds": time.monotonic() - started,
                        "lastEnqueuedSensorUsec": last_published_sensor_usec,
                        "actuatorTimeUsec": observation.time_usec if observation else None,
                        "actuatorFlags": observation.flags if observation else None,
                        "actuatorReceiveSequence": observation.receive_sequence if observation else None,
                        "actuatorReceivedMonotonicNs": observation.received_monotonic_ns if observation else None,
                    })
                    check_truth(packet, controls, hil_step)
                    return packet

                def emit(packet: np.ndarray, hil_step: int) -> None:
                    nonlocal last_published_sensor_usec
                    frame, _ = decode_hil_packet(
                        packet, time_usec=round(hil_step * 1_000_000 / profile.hil_hz),
                        fields_updated=profile.cadence.fields_updated(profile.hil_hz, hil_step),
                        gps_updated=profile.cadence.gps_due(profile.hil_hz, hil_step),
                    )
                    bridge.publish(hil_step, frame)
                    last_published_sensor_usec = frame.time_usec
                    bridge.raise_if_failed()

                if schedule == "paced":
                    packet = None
                    for step in range(1, profile.hil_hz * 540 + 1):
                        if stop.is_set():
                            return
                        plant_step = profile.plant_step(step)
                        if plant_step is not None:
                            packet = sample(plant_step, step)
                        emit(packet, step)
                        stop.wait(max(0.0, started + step / profile.hil_hz - time.monotonic()))
                else:
                    clock = MonotonicPhysicsClock(
                        profile.physics_hz, maximum_steps_per_pass=profile.physics_hz,
                    )
                    clock.reset(0, now=started)
                    plant_step = 0
                    stalled_phases: set[str] = set()
                    while time.monotonic() - started < 540:
                        if stop.is_set():
                            return
                        current_phase = phase[0]
                        if current_phase.startswith("mission ") and current_phase not in stalled_phases:
                            stalled_phases.add(current_phase)
                            stall_start = time.monotonic()
                            if stop.wait(0.5):
                                return
                            stall_observations.append({
                                "phase": current_phase, "plantStep": plant_step,
                                "wallStartSeconds": stall_start - started,
                                "wallEndSeconds": time.monotonic() - started,
                            })
                        due = clock.due_steps(plant_step)
                        if due:
                            catch_up_passes.append({
                                "firstPlantStep": plant_step + 1, "dueSteps": due,
                                "wallSeconds": time.monotonic() - started,
                            })
                        for _ in range(due):
                            if stop.is_set():
                                return
                            plant_step += 1
                            first_hil_step = (plant_step - 1) * 2 + 1
                            packet = sample(plant_step, first_hil_step)
                            # Match fleet_runtime.step: one plant sample, then BOTH frames
                            # enqueued immediately, with no wait for actuator feedback.
                            emit(packet, first_hil_step)
                            emit(packet, first_hil_step + 1)
                        if due == 0:
                            stop.wait(min(clock.seconds_until_next_step(plant_step), 0.005))
                raise TimeoutError("CUDA flight publisher exceeded 540 seconds")
            except BaseException as error:
                failures.append(error)
                print(json.dumps({"phase": phase[0], "profile": profile.name,
                                  "publisherFailure": str(error)}), flush=True)
                stop.set()

        def topic(name: str) -> str:
            return subprocess.run(
                [str(root / "build/px4_sitl_default/bin/px4-listener"),
                 "--instance", str(instance), name, "-n", "1"],
                capture_output=True, text=True, timeout=5.0, check=True,
            ).stdout

        def wait_for(label: str, seconds: float, predicate) -> None:
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                self.assertFalse(failures, str(failures))
                bridge.raise_if_failed()
                commander.status()  # Keep the production GCS heartbeat active.
                if predicate():
                    observation = {"phase": label, "position_enu_m": latest[0][:3].tolist()}
                    observations.append(observation)
                    print(json.dumps(observation), flush=True)
                    return
                self.assertFalse(stop.wait(0.25), str(failures))
            self.fail(f"{label} timed out; {topic('vehicle_status')}; {topic('estimator_status_flags')}")

        producer = threading.Thread(target=publish, name="px4-flight-cuda", daemon=True)
        try:
            bridge.start()
            producer.start()
            commander.connect(timeout_seconds=15.0)
            wait_for("initial preflight", 60.0, lambda: "pre_flight_checks_pass: True" in topic("vehicle_status"))
            for name, expected in (("IMU_INTEG_RATE", PX4_IRIS_IMU_NOISE_REFERENCE_HZ),
                                   ("IMU_GYRO_RATEMAX", 800)):
                value = subprocess.run(
                    [str(root / "build/px4_sitl_default/bin/px4-param"),
                     "--instance", str(instance), "show", name],
                    capture_output=True, text=True, timeout=5.0, check=True,
                ).stdout
                self.assertRegex(value, rf"{name}.*:\s*{expected}\s", value)
            for cycle in range(2):
                phase[:] = [f"takeoff {cycle + 1}"]
                commander.takeoff(8.0, deadline=CommandDeadline.after(30.0))
                wait_for(f"takeoff {cycle + 1}", 45.0, lambda: bool(latest) and latest[0][2] > 6.0)
                # Changes in horizontal velocity make EKF yaw observable against GPS.
                route = tuple(
                    Waypoint(40.758 + north / 111_000.0, -73.9855 + east / 84_000.0,
                             -9.0, 5.0, 1.0)
                    for east, north in ((40.0, 0.0), (40.0, 40.0), (0.0, 0.0))
                )
                phase[:] = [f"mission {cycle + 1}"]
                self.assertEqual(commander.execute_mission(route, timeout_seconds=100.0), len(route))
                self.assertIn("cs_mag_fault: False", topic("estimator_status_flags"))
                phase[:] = [f"landing {cycle + 1}"]
                commander.land(deadline=CommandDeadline.after(20.0))
                wait_for(f"landed {cycle + 1}", 60.0,
                         lambda: commander.status().flight_state in {"landed", "standby"}
                         and bool(latest) and latest[0][2] < 0.1)
                wait_for(f"postflight preflight {cycle + 1}", 20.0,
                         lambda: "pre_flight_checks_pass: True" in topic("vehicle_status"))
                self.assertIn("cs_mag_fault: False", topic("estimator_status_flags"))
                if cycle == 0:
                    # This commander has no IN_AIR history. Preflight can send
                    # Land after touchdown; reproduce that observed PX4 mode
                    # before asking the newly connected commander to rearm.
                    commander.close()
                    commander = Px4Commander(instance=instance, origin_height_m=-17.0)
                    commander.connect(timeout_seconds=15.0)
                    wait_for("grounded commander reconnect", 10.0,
                             lambda: commander.status().flight_state == "standby"
                             and commander._landed_state == mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND)
                    self.assertFalse(commander._has_flown)
                    commander.land(deadline=CommandDeadline.after(20.0))
                    wait_for("grounded Land mode", 10.0,
                             lambda: (commander._px4_main_mode, commander._px4_sub_mode)
                             == mavutil.px4_map["LAND"][1:])
            self.assertFalse(failures, str(failures))
            if schedule == "grouped-catch-up":
                self.assertEqual([row["phase"] for row in stall_observations],
                                 ["mission 1", "mission 2"])
        finally:
            # Simulator process teardown ends any unresolved simulated operation;
            # this fixture never shares a vehicle with an installation or another test.
            stop.set()
            if producer.ident is not None:
                producer.join(timeout=5.0)
            try:
                if capture:
                    # These isolated simulation logs contain no installation credentials.
                    destination = Path(capture)
                    destination.mkdir(parents=True, exist_ok=True)
                    np.save(destination / "hil-samples.npy", np.asarray(samples))
                    np.save(destination / "motor-rotor-speed-rad-s.npy", np.asarray(control_samples))
                    np.save(destination / "sample-timing.npy", np.asarray(sample_timing))
                    (destination / "catch-up-passes.jsonl").write_text(
                        "".join(json.dumps(row) + "\n" for row in catch_up_passes)
                    )
                    (destination / "feedback-timing.jsonl").write_text(
                        "".join(json.dumps(row) + "\n" for row in feedback_timing)
                    )
                    (destination / "flight-diagnostics.json").write_text(json.dumps({
                        "profile": profile.name, "schedule": schedule, "stalls": stall_observations,
                        "physicsHz": profile.physics_hz,
                        "hilHz": profile.hil_hz, "samples": len(samples),
                        "motorCommandUnit": "rad/s",
                        "saturationNormalizedThresholds": [0.01, 0.99],
                        "saturationSecondsPerMotor": saturation_seconds.tolist(),
                        "longestSaturationSecondsPerMotor": longest_saturation.tolist(),
                        "maximumObserved": maxima, "lastPhase": phase[0],
                        "groundContactExclusionUpM": 0.10,
                        "maxTiltDegrees": 60, "maxBodyRateDegreesPerSecond": 360,
                        "failures": [str(error) for error in failures],
                    }, indent=2) + "\n")
                    for name in ("vehicle_attitude", "vehicle_local_position",
                                 "estimator_status_flags", "estimator_sensor_bias",
                                 "yaw_estimator_status", "vehicle_status"):
                        try:
                            (destination / f"{name}.txt").write_text(topic(name))
                        except (OSError, subprocess.SubprocessError) as error:
                            print(f"diagnostic topic {name} unavailable: {error}", flush=True)
                    # Reap PX4 before copying: its logger must finish writing while the root exists.
                    exit_code = bridge._process.stop()
                    (destination / "px4-exit.json").write_text(json.dumps({
                        "exitCode": exit_code, "reapedBeforeLogCopy": True,
                    }) + "\n")
                    for log in bridge._process.command.working_directory.glob("log/**/*.ulg"):
                        shutil.copyfile(log, destination / log.name)
            finally:
                commander.close()
                bridge.close()
            self.assertFalse(producer.is_alive(), "CUDA flight publisher did not terminate")
            # The last in-flight packet may fail after the pre-finally assertion.
            self.assertFalse(failures, str(failures))


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(Px4FlightTests)
    )
    print(json.dumps({
        "suite": "uav-native-px4-flight", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
