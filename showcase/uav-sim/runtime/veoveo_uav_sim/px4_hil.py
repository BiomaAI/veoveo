from __future__ import annotations

import os
import signal
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from pymavlink import mavutil

from .vehicle_spec import HilSensorFrame, decode_actuator_controls

PX4_EXTERNAL_IRIS_AUTOSTART = "10016"


@dataclass(frozen=True, slots=True)
class Px4ProcessCommand:
    executable: Path
    romfs: Path
    startup_script: Path
    instance: int
    working_directory: Path

    def argv(self) -> tuple[str, ...]:
        return (
            str(self.executable),
            str(self.romfs),
            "-s",
            str(self.startup_script),
            "-i",
            str(self.instance),
            "-d",
        )


class Px4Process:
    """Own one PX4 SITL process and its isolated writable root."""

    def __init__(self, px4_directory: str, instance: int) -> None:
        root = Path(px4_directory)
        self._temporary_root = tempfile.TemporaryDirectory(
            prefix=f"veoveo-px4-{instance}-"
        )
        self.command = Px4ProcessCommand(
            executable=root / "build/px4_sitl_default/bin/px4",
            romfs=root / "ROMFS/px4fmu_common",
            startup_script=root / "ROMFS/px4fmu_common/init.d-posix/rcS",
            instance=instance,
            working_directory=Path(self._temporary_root.name),
        )
        self._process: subprocess.Popen[bytes] | None = None

    def start(self) -> None:
        if self._process is not None:
            raise RuntimeError("PX4 process has already been started")
        for required in (
            self.command.executable,
            self.command.romfs,
            self.command.startup_script,
        ):
            if not required.exists():
                raise RuntimeError(f"PX4 runtime input does not exist: {required}")
        environment = os.environ.copy()
        environment["PX4_SYS_AUTOSTART"] = PX4_EXTERNAL_IRIS_AUTOSTART
        self._process = subprocess.Popen(
            self.command.argv(),
            cwd=self.command.working_directory,
            env=environment,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )

    def raise_if_failed(self) -> None:
        if self._process is None:
            raise RuntimeError("PX4 process has not been started")
        status = self._process.poll()
        if status is not None:
            raise RuntimeError(
                f"PX4 instance {self.command.instance} exited with status {status}"
            )

    def close(self) -> None:
        process = self._process
        self._process = None
        if process is not None and process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5.0)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5.0)
        self._temporary_root.cleanup()


class Px4HilBridge:
    """One concurrent MAVLink HIL worker with a latest-frame exchange slot."""

    def __init__(self, px4_directory: str, instance: int) -> None:
        self.instance = instance
        self._process = Px4Process(px4_directory, instance)
        self._condition = threading.Condition()
        self._stop = False
        self._pending: tuple[int, HilSensorFrame] | None = None
        self._sent_sequence = 0
        self._actuator_generation = 0
        self._controls = (0.0, 0.0, 0.0, 0.0)
        self._heartbeat_received = False
        self._failure: BaseException | None = None
        self._connection: Any = None
        self._thread: threading.Thread | None = None

    @property
    def connected(self) -> bool:
        with self._condition:
            return self._heartbeat_received and self._failure is None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("PX4 HIL bridge has already been started")
        self._connection = mavutil.mavlink_connection(
            f"tcpin:127.0.0.1:{4560 + self.instance}"
        )
        self._process.start()
        self._thread = threading.Thread(
            target=self._run,
            name=f"px4-hil-{self.instance}",
            daemon=True,
        )
        self._thread.start()

    def controls(self) -> tuple[float, float, float, float]:
        with self._condition:
            self._raise_if_failed_locked()
            return self._controls

    def publish(self, sequence: int, frame: HilSensorFrame) -> int:
        with self._condition:
            self._raise_if_failed_locked()
            if self._pending is not None:
                raise RuntimeError(
                    f"PX4 HIL instance {self.instance} still owns an unsent frame"
                )
            prior_actuator_generation = self._actuator_generation
            self._pending = (sequence, frame)
            self._condition.notify_all()
            return prior_actuator_generation

    def wait_exchange(
        self,
        sequence: int,
        prior_actuator_generation: int,
        deadline: float,
    ) -> None:
        with self._condition:
            while True:
                self._raise_if_failed_locked()
                sent = self._sent_sequence >= sequence
                actuator_ready = (
                    prior_actuator_generation == 0
                    or self._actuator_generation > prior_actuator_generation
                )
                if sent and actuator_ready:
                    return
                remaining = deadline - time.monotonic()
                if remaining <= 0.0:
                    waiting_for = "actuator response" if sent else "sensor send"
                    raise TimeoutError(
                        f"PX4 HIL instance {self.instance} timed out waiting for "
                        f"{waiting_for} at sequence {sequence}"
                    )
                self._condition.wait(remaining)

    def raise_if_failed(self) -> None:
        self._process.raise_if_failed()
        with self._condition:
            self._raise_if_failed_locked()

    def close(self) -> None:
        with self._condition:
            self._stop = True
            self._condition.notify_all()
        if self._thread is not None:
            self._thread.join(timeout=5.0)
            self._thread = None
        if self._connection is not None:
            self._connection.close()
            self._connection = None
        self._process.close()

    def _run(self) -> None:
        try:
            last_heartbeat = 0.0
            while True:
                with self._condition:
                    if self._stop:
                        return
                    pending = self._pending
                    self._pending = None
                self._process.raise_if_failed()
                self._drain_messages()
                now = time.monotonic()
                if now - last_heartbeat >= 1.0:
                    self._send_heartbeat()
                    last_heartbeat = now
                if pending is not None:
                    sequence, frame = pending
                    self._send_frame(frame)
                    with self._condition:
                        self._sent_sequence = sequence
                        self._condition.notify_all()
                    self._drain_messages()
                else:
                    with self._condition:
                        self._condition.wait(0.001)
        except BaseException as error:
            with self._condition:
                self._failure = error
                self._condition.notify_all()

    def _drain_messages(self) -> None:
        for _ in range(256):
            message = self._connection.recv_match(blocking=False)
            if message is None:
                return
            message_type = message.get_type()
            with self._condition:
                if message_type == "HEARTBEAT":
                    self._heartbeat_received = True
                elif message_type == "HIL_ACTUATOR_CONTROLS":
                    self._controls = decode_actuator_controls(
                        message.controls,
                        int(message.mode),
                        mavutil.mavlink.MAV_MODE_FLAG_SAFETY_ARMED,
                    )
                    self._actuator_generation += 1
                self._condition.notify_all()

    def _send_heartbeat(self) -> None:
        self._connection.mav.heartbeat_send(
            mavutil.mavlink.MAV_TYPE_GENERIC,
            mavutil.mavlink.MAV_AUTOPILOT_INVALID,
            0,
            0,
            0,
        )

    def _send_frame(self, frame: HilSensorFrame) -> None:
        acceleration = frame.acceleration_frd_mps2
        angular_velocity = frame.angular_velocity_frd_rps
        magnetic_field = frame.magnetic_field_frd_gauss
        self._connection.mav.hil_sensor_send(
            frame.time_usec,
            *acceleration,
            *angular_velocity,
            *magnetic_field,
            frame.absolute_pressure_hpa,
            frame.differential_pressure_hpa,
            frame.pressure_altitude_m,
            frame.temperature_celsius,
            frame.fields_updated,
        )
        if frame.gps_updated:
            north, east, down = frame.velocity_ned_mps
            self._connection.mav.hil_gps_send(
                frame.time_usec,
                frame.fix_type,
                int(round(frame.latitude_degrees * 10_000_000.0)),
                int(round(frame.longitude_degrees * 10_000_000.0)),
                int(round(frame.altitude_m * 1000.0)),
                int(round(frame.eph_m * 100.0)),
                int(round(frame.epv_m * 100.0)),
                int(round(frame.ground_speed_mps * 100.0)),
                int(round(north * 100.0)),
                int(round(east * 100.0)),
                int(round(down * 100.0)),
                int(round(frame.course_over_ground_degrees * 100.0)) % 36_000,
                frame.satellites_visible,
            )

    def _raise_if_failed_locked(self) -> None:
        if self._failure is not None:
            raise RuntimeError(
                f"PX4 HIL worker {self.instance} failed"
            ) from self._failure


class Px4HilFleet:
    """Launch and exchange every PX4 HIL endpoint concurrently."""

    def __init__(self, px4_directory: str, vehicle_count: int) -> None:
        if vehicle_count < 1:
            raise ValueError("PX4 HIL fleet must contain at least one vehicle")
        self._bridges = tuple(
            Px4HilBridge(px4_directory, instance) for instance in range(vehicle_count)
        )
        self._sequence = 0

    @property
    def vehicle_count(self) -> int:
        return len(self._bridges)

    def start(self) -> None:
        started: list[Px4HilBridge] = []
        try:
            for bridge in self._bridges:
                bridge.start()
                started.append(bridge)
        except BaseException:
            for bridge in reversed(started):
                bridge.close()
            raise

    def controls(self) -> tuple[tuple[float, float, float, float], ...]:
        return tuple(bridge.controls() for bridge in self._bridges)

    def exchange(self, frames: tuple[HilSensorFrame, ...], timeout: float) -> None:
        if len(frames) != len(self._bridges):
            raise ValueError("PX4 HIL frame count does not match the fleet")
        if timeout <= 0.0:
            raise ValueError("PX4 HIL exchange timeout must be positive")
        self._sequence += 1
        generations = tuple(
            bridge.publish(self._sequence, frame)
            for bridge, frame in zip(self._bridges, frames, strict=True)
        )
        deadline = time.monotonic() + timeout
        for bridge, generation in zip(self._bridges, generations, strict=True):
            bridge.wait_exchange(self._sequence, generation, deadline)

    def raise_if_failed(self) -> None:
        for bridge in self._bridges:
            bridge.raise_if_failed()

    def close(self) -> None:
        for bridge in reversed(self._bridges):
            bridge.close()
